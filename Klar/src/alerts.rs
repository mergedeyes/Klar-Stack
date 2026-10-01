//! Alerts for admins, so nothing that needs a person waits until someone
//! happens to open an admin page.
//!
//! - At once: a report that can't wait -- child sexual abuse material,
//!   intimate images shared without consent, terrorism (a removal order for
//!   terrorist content allows one hour, Regulation 2021/784) -- and a
//!   removal the catalog says must be reported to the authorities (DSA
//!   Art. 18: "promptly").
//! - Once a day, from 06:00 UTC: how many reports, objections, rights
//!   claims, held-back statements and overdue items are waiting.
//!
//! Emails go to every admin address (ADMIN_EMAILS) with a verified account.
//! They carry counts and a link, never content or names: an email leaves our
//! systems, the admin pages don't. Each alert is claimed in admin_alerts
//! before it is sent, so replicas don't send the same one twice.

use std::time::Duration;

use chrono::{Timelike, Utc};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::handlers::auth::AppState;
use crate::utils::{admin_emails, DbResultExt};

/// Report reasons an admin is alerted about at once.
pub fn is_urgent(reason: &str) -> bool {
    matches!(reason, "csam" | "ncii" | "terrorism")
}

pub enum Alert {
    /// A report for an urgent reason. One alert per item and reason.
    UrgentReport { target_type: String, target_id: Uuid, reason: String },
    /// A removal classified as a violation that must be reported to the
    /// authorities, with its evidence record if one exists.
    AuthorityReportRequired { target_type: String, target_id: Uuid, evidence_id: Option<Uuid> },
}

fn reason_de(reason: &str) -> &'static str {
    match reason {
        "csam" => "Darstellung sexuellen Missbrauchs von Minderjährigen",
        "ncii" => "intime Aufnahmen ohne Einwilligung",
        "terrorism" => "Terrorismus oder Androhung schwerer Gewalt",
        _ => "dringender Meldegrund",
    }
}

/// Claims an alert in admin_alerts. False if it was sent before.
async fn claim(db: &PgPool, kind: &str, key: &str) -> Result<bool, AppError> {
    sqlx::query("INSERT INTO admin_alerts (kind, key) VALUES ($1, $2) ON CONFLICT DO NOTHING")
        .bind(kind)
        .bind(key)
        .execute(db)
        .await
        .db_err("Database error")
        .map(|r| r.rows_affected() > 0)
}

/// The verified accounts behind the ADMIN_EMAILS addresses.
async fn recipients(db: &PgPool) -> Result<Vec<String>, AppError> {
    sqlx::query_scalar::<_, String>("SELECT email FROM users WHERE email_verified AND LOWER(email) = ANY($1)")
        .bind(admin_emails())
        .fetch_all(db)
        .await
        .db_err("Database error")
}

/// Sends an alert to every admin, once. Runs after the commit of whatever
/// raised it; failures are logged (the item is in the admin pages anyway).
pub async fn send(state: &AppState, alert: Alert) {
    let (kind, key, subject, message, path) = match &alert {
        Alert::UrgentReport { target_type, target_id, reason } => (
            "urgent_report",
            format!("{target_type}:{target_id}:{reason}"),
            "Klar: dringende Meldung",
            format!(
                "Eine neue Meldung wegen „{}“ wartet auf Prüfung. Bitte sieh sie dir so bald wie möglich an.",
                reason_de(reason)
            ),
            "/admin/reports".to_string(),
        ),
        Alert::AuthorityReportRequired { target_type, target_id, evidence_id } => (
            "authority_report",
            format!("{target_type}:{target_id}"),
            "Klar: Meldung an Behörden erforderlich",
            "Ein entfernter Inhalt ist als Straftat eingestuft, die Leben oder Sicherheit von Menschen gefährdet. \
             Er muss unverzüglich den Strafverfolgungsbehörden gemeldet werden (Art. 18 DSA); trag die Meldung danach \
             im Beweismittel ein."
                .to_string(),
            evidence_id.map(|id| format!("/admin/evidence/{id}")).unwrap_or_else(|| "/admin/evidence".to_string()),
        ),
    };
    let state = state.clone();
    tokio::spawn(async move {
        match claim(&state.db, kind, &key).await {
            Ok(true) => {}
            Ok(false) => return,
            Err(e) => {
                tracing::error!("Admin alert {kind} {key}: claiming failed: {}", e.message);
                return;
            }
        }
        deliver(&state, subject, &message, &path).await;
    });
}

async fn deliver(state: &AppState, subject: &str, message: &str, path: &str) {
    let to = match recipients(&state.db).await {
        Ok(to) => to,
        Err(e) => {
            tracing::error!("Admin alert: listing admins failed: {}", e.message);
            return;
        }
    };
    if to.is_empty() {
        tracing::warn!("Admin alert \"{subject}\" not sent: no verified admin account");
    }
    for email in to {
        if let Err(e) = state.email.send_admin_alert(&email, subject, message, path).await {
            tracing::error!("Admin alert \"{subject}\" failed: {}", e.0);
        }
    }
}

/// What is waiting for an admin: the daily digest, and the badges on the
/// admin entries in Settings.
#[derive(Debug, Default, Serialize, sqlx::FromRow)]
pub struct Attention {
    /// Items (posts, comments, messages, profiles) with pending reports.
    pub reports: i64,
    /// ... of which with an urgent reason.
    pub urgent_reports: i64,
    /// ... of which waiting for more than 30 days.
    pub overdue_reports: i64,
    pub objections: i64,
    pub rights_claims: i64,
    /// Statements held back (CSAM) until an admin sends them, and those
    /// held for more than a week.
    pub held_statements: i64,
    pub overdue_held_statements: i64,
    /// Evidence undecided for more than 30 days.
    pub overdue_evidence: i64,
    /// Required reports to the authorities (Art. 18) not recorded yet.
    pub authority_reports: i64,
}

impl Attention {
    pub fn total(&self) -> i64 {
        self.reports + self.objections + self.rights_claims + self.held_statements + self.overdue_evidence
            + self.authority_reports
    }
}

pub async fn attention(db: &PgPool) -> Result<Attention, AppError> {
    sqlx::query_as::<_, Attention>(
        r#"
        SELECT
            (SELECT COUNT(DISTINCT (target_type, target_id)) FROM reports WHERE status = 'pending') AS reports,
            (SELECT COUNT(DISTINCT (target_type, target_id)) FROM reports
             WHERE status = 'pending' AND reason IN ('csam', 'ncii', 'terrorism')) AS urgent_reports,
            (SELECT COUNT(DISTINCT (target_type, target_id)) FROM reports
             WHERE status = 'pending' AND created_at < NOW() - INTERVAL '30 days') AS overdue_reports,
            (SELECT COUNT(*) FROM moderation_decisions WHERE objection_status = 'pending') AS objections,
            (SELECT COUNT(*) FROM rights_claims WHERE status IN ('submitted', 'triaged', 'evidence_requested')) AS rights_claims,
            (SELECT COUNT(*) FROM moderation_decisions
             WHERE delivered_at IS NULL AND affected_user_id IS NOT NULL AND lifted_at IS NULL AND superseded_by IS NULL)
                AS held_statements,
            (SELECT COUNT(*) FROM moderation_decisions
             WHERE delivered_at IS NULL AND affected_user_id IS NOT NULL AND lifted_at IS NULL AND superseded_by IS NULL
               AND created_at < NOW() - INTERVAL '7 days') AS overdue_held_statements,
            (SELECT COUNT(*) FROM evidence_records
             WHERE decided_at IS NULL AND purged_at IS NULL AND created_at < NOW() - INTERVAL '30 days') AS overdue_evidence,
            (SELECT COUNT(*) FROM evidence_records e
             WHERE e.purged_at IS NULL AND e.authority_report = 'required'
               AND NOT EXISTS (SELECT 1 FROM evidence_events ev WHERE ev.evidence_id = e.id AND ev.action = 'authority_report'))
                AS authority_reports
        "#,
    )
    .fetch_one(db)
    .await
    .db_err("Database error")
}

/// The digest's lines, only for what is non-zero.
fn digest_lines(a: &Attention) -> Vec<String> {
    let mut lines = Vec::new();
    if a.reports > 0 {
        let mut line = format!("{} gemeldete Inhalte warten auf eine Entscheidung", a.reports);
        let mut extra = Vec::new();
        if a.urgent_reports > 0 {
            extra.push(format!("{} dringend", a.urgent_reports));
        }
        if a.overdue_reports > 0 {
            extra.push(format!("{} seit über 30 Tagen", a.overdue_reports));
        }
        if !extra.is_empty() {
            line = format!("{line} ({})", extra.join(", "));
        }
        lines.push(line);
    }
    if a.objections > 0 {
        lines.push(format!("{} Widersprüche warten auf eine Antwort", a.objections));
    }
    if a.rights_claims > 0 {
        lines.push(format!("{} Rechte-Meldungen sind offen", a.rights_claims));
    }
    if a.held_statements > 0 {
        let overdue = if a.overdue_held_statements > 0 {
            format!(" ({} seit über einer Woche)", a.overdue_held_statements)
        } else {
            String::new()
        };
        lines.push(format!("{} zurückgehaltene Begründungen sind noch nicht versandt{}", a.held_statements, overdue));
    }
    if a.overdue_evidence > 0 {
        lines.push(format!("{} Beweismittel sind seit über 30 Tagen unentschieden", a.overdue_evidence));
    }
    if a.authority_reports > 0 {
        lines.push(format!("{} erforderliche Meldungen an Behörden sind noch nicht eingetragen", a.authority_reports));
    }
    lines
}

/// Sends today's digest if it is due and anything is waiting.
pub(crate) async fn send_digest_if_due(state: &AppState) {
    let now = Utc::now();
    if now.hour() < 6 {
        return;
    }
    let attention = match attention(&state.db).await {
        Ok(a) => a,
        Err(e) => {
            tracing::error!("Admin digest: counting failed: {}", e.message);
            return;
        }
    };
    let lines = digest_lines(&attention);
    if lines.is_empty() {
        return;
    }
    match claim(&state.db, "digest", &now.date_naive().to_string()).await {
        Ok(true) => {}
        Ok(false) => return,
        Err(e) => {
            tracing::error!("Admin digest: claiming failed: {}", e.message);
            return;
        }
    }
    let message = format!("Stand heute:\n\n- {}", lines.join("\n- "));
    deliver(state, "Klar: Was heute auf dich wartet", &message, "/admin/reports").await;
}

/// Checks every 15 minutes whether the digest is due; also clears alert
/// claims older than 30 days. Safe on every replica.
pub fn spawn(state: AppState) {
    tokio::spawn(async move {
        loop {
            send_digest_if_due(&state).await;
            if let Err(e) = sqlx::query("DELETE FROM admin_alerts WHERE sent_at < NOW() - INTERVAL '30 days'")
                .execute(&state.db)
                .await
            {
                tracing::error!("Admin alert cleanup failed: {}", e);
            }
            tokio::time::sleep(Duration::from_secs(15 * 60)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digest_mentions_only_what_is_waiting() {
        assert!(digest_lines(&Attention::default()).is_empty());
        let a = Attention { reports: 3, urgent_reports: 1, objections: 2, ..Default::default() };
        let lines = digest_lines(&a);
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("3 gemeldete Inhalte") && lines[0].contains("1 dringend"));
        assert!(is_urgent("csam") && is_urgent("terrorism") && !is_urgent("spam"));
    }
}
