//! Statements of reasons (DSA Art. 17) and report outcomes (Art. 16).
//!
//! Whenever someone's post or comment is restricted -- removed by an admin,
//! or automatically hidden / shown behind a warning after a report --
//! `record_decision` stores a decision record. Delivering it notifies the
//! author in the app and by email, linking to the statement at
//! /moderation/decisions/:id, where they can also object. Reporters learn
//! the outcome of their report through `notify_report_outcome`.
//!
//! The statement texts (ground cited, explanation) are built here, in one
//! place, and stored exactly as shown. ⚖️ Wording and the reason-to-ground
//! mapping are pending legal review.

use serde_json::json;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::errors::AppError;
use crate::handlers::auth::AppState;
use crate::handlers::notifications::{publish_notification, NotificationEvent, NotificationResponse};
use crate::utils::DbResultExt;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Restriction {
    Removed,
    Hidden,
    Flagged,
}

impl Restriction {
    pub fn as_str(self) -> &'static str {
        match self {
            Restriction::Removed => "removed",
            Restriction::Hidden => "hidden",
            Restriction::Flagged => "flagged",
        }
    }
}

/// Statements for CSAM are held back until an admin releases them, so a
/// suspect isn't alerted before the content has been reviewed and, where
/// needed, reported to the authorities. ⚖️ Pending legal review.
fn held_back(reason: &str) -> bool {
    reason == "csam"
}

/// The ground a restriction for this report reason rests on: whether it's
/// a legal provision or our Terms of Service, and which one.
fn ground_for(reason: &str) -> (&'static str, &'static str, &'static str) {
    match reason {
        "csam" => (
            "illegal",
            "§ 184b StGB (Verbreitung kinderpornographischer Inhalte); Nutzungsbedingungen Abschnitt 4",
            "Der Inhalt wurde als Darstellung sexuellen Missbrauchs von Minderjährigen gemeldet.",
        ),
        "violence" => (
            "terms",
            "Nutzungsbedingungen Abschnitt 4 (Gewaltdarstellungen)",
            "Der Inhalt wurde als Gewaltdarstellung oder verstörender Inhalt gemeldet.",
        ),
        "hate_speech" => (
            "terms",
            "Nutzungsbedingungen Abschnitt 4 (rechtswidrige Inhalte, u. a. Volksverhetzung)",
            "Der Inhalt wurde als Hassrede gemeldet.",
        ),
        "harassment" => (
            "terms",
            "Nutzungsbedingungen Abschnitt 4 (Belästigung, Mobbing, Bedrohung oder Stalking)",
            "Der Inhalt wurde als Belästigung oder Mobbing gemeldet.",
        ),
        // ⚖️ Abschnitt 4 doesn't list these two explicitly yet; Abschnitt 5
        // covers restricting reported content.
        "self_harm" => (
            "terms",
            "Nutzungsbedingungen Abschnitt 5 (Maßnahmen nach Meldungen)",
            "Der Inhalt wurde als Darstellung von Selbstverletzung oder Suizid gemeldet.",
        ),
        "sexual_content" => (
            "terms",
            "Nutzungsbedingungen Abschnitt 5 (Maßnahmen nach Meldungen)",
            "Der Inhalt wurde als sexueller Inhalt gemeldet.",
        ),
        "spam" => (
            "terms",
            "Nutzungsbedingungen Abschnitt 4 (Spam)",
            "Der Inhalt wurde als Spam gemeldet.",
        ),
        "copyright" => (
            "illegal",
            "§ 97 UrhG (Verletzung von Urheber- oder verwandten Schutzrechten); Nutzungsbedingungen Abschnitt 3",
            "Eine Person, die Rechte an einem Werk geltend macht, hat gemeldet, dass der Inhalt dieses Werk ohne Erlaubnis verwendet.",
        ),
        "impersonation" => (
            "terms",
            "Nutzungsbedingungen Abschnitt 4 (Vortäuschen falscher Identitäten)",
            "Der Inhalt wurde als Identitätsdiebstahl gemeldet.",
        ),
        _ => (
            "terms",
            "Nutzungsbedingungen Abschnitt 4",
            "Der Inhalt wurde wegen eines Verstoßes gegen unsere Nutzungsbedingungen gemeldet.",
        ),
    }
}

fn restriction_text(restriction: Restriction, automated: bool) -> &'static str {
    match restriction {
        Restriction::Removed => "Unser Team hat den Inhalt geprüft und entfernt.",
        Restriction::Hidden if !automated => {
            "Unser Team hat die Meldung geprüft und den Inhalt ausgeblendet. Er wird wiederhergestellt, \
             wenn dein Widerspruch Erfolg hat."
        }
        Restriction::Hidden => {
            "Der Inhalt wurde nach der Meldung automatisch ausgeblendet, bis unser Team ihn geprüft hat."
        }
        Restriction::Flagged => {
            "Der Inhalt wird nach der Meldung automatisch nur noch mit einem Warnhinweis angezeigt, \
             bis unser Team ihn geprüft hat."
        }
    }
}

const EXCERPT_CHARS: usize = 200;

fn excerpt(text: Option<String>) -> Option<String> {
    let text = text?.trim().to_string();
    if text.is_empty() {
        return None;
    }
    if text.chars().count() <= EXCERPT_CHARS {
        Some(text)
    } else {
        Some(format!("{}…", text.chars().take(EXCERPT_CHARS).collect::<String>()))
    }
}

/// Notices to send once the caller's transaction has committed.
#[must_use]
#[derive(Default)]
pub struct PendingNotices {
    events: Vec<NotificationEvent>,
    /// Statement emails: (recipient's email, decision id).
    emails: Vec<(String, Uuid)>,
}

impl PendingNotices {
    pub fn extend(&mut self, other: PendingNotices) {
        self.events.extend(other.events);
        self.emails.extend(other.emails);
    }

    /// Pushes the live notifications and sends the statement emails. Runs
    /// after commit; failures are logged, since the notices are stored and
    /// show up in the app regardless.
    pub async fn send(self, state: &AppState) {
        for event in self.events {
            publish_notification(state, &event).await;
        }
        for (email, decision_id) in self.emails {
            let state = state.clone();
            tokio::spawn(async move {
                if let Err(e) = state.email.send_moderation_notice(&email, decision_id).await {
                    tracing::error!("Statement email for decision {} failed: {}", decision_id, e.0);
                }
            });
        }
    }
}

/// Stores a notification from Klar itself (no acting user) and returns the
/// live event for it.
async fn insert_system_notification(
    conn: &mut PgConnection,
    user_id: Uuid,
    kind: &str,
    decision_id: Option<Uuid>,
) -> Result<NotificationEvent, AppError> {
    let id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO notifications (user_id, actor_id, type, decision_id) VALUES ($1, NULL, $2::notification_type, $3) RETURNING id",
    )
    .bind(user_id)
    .bind(kind)
    .bind(decision_id)
    .fetch_one(&mut *conn)
    .await
    .db_err_ctx("Failed to store moderation notice", "Database error")?;

    Ok(NotificationEvent {
        target_user_id: user_id,
        notification: NotificationResponse {
            id,
            type_name: kind.to_string(),
            is_read: false,
            created_at: chrono::Utc::now(),
            actor: None,
            post_id: None,
            post_thumb_url: None,
            decision_id,
        },
    })
}

pub struct NewDecision<'a> {
    pub target_type: &'a str,
    pub target_id: Uuid,
    pub restriction: Restriction,
    pub automated: bool,
    pub reason: &'a str,
    pub decided_by: Option<Uuid>,
    /// The report behind it, or the rights claim (rights.rs).
    pub report_id: Option<Uuid>,
    pub rights_claim_id: Option<Uuid>,
    /// Appended to the explanation, e.g. which work a rights claim is about.
    pub detail: Option<String>,
}

/// Records a restriction of a post or comment, inside the transaction that
/// imposes it and before any delete (the author and excerpt are read from
/// the content). A repeat of an active restriction (a second report that
/// would flag an already-flagged post) only adds the report to it. A
/// removal supersedes earlier automatic restrictions.
pub async fn record_decision(tx: &mut PgConnection, d: NewDecision<'_>) -> Result<PendingNotices, AppError> {
    let mut pending = PendingNotices::default();

    let content = match d.target_type {
        "post" => sqlx::query_as::<_, (Uuid, Option<String>)>("SELECT user_id, caption FROM posts WHERE id = $1")
            .bind(d.target_id)
            .fetch_optional(&mut *tx)
            .await,
        "comment" => sqlx::query_as::<_, (Uuid, Option<String>)>("SELECT user_id, body FROM comments WHERE id = $1")
            .bind(d.target_id)
            .fetch_optional(&mut *tx)
            .await,
        _ => return Ok(pending),
    }
    .db_err("Database error")?;
    // Already gone (its author deleted it): nothing is restricted any more.
    let Some((affected_user_id, text)) = content else {
        return Ok(pending);
    };

    let repeat = sqlx::query_scalar::<_, Uuid>(
        r#"
        UPDATE moderation_decisions SET report_ids = CASE WHEN $4::uuid IS NULL THEN report_ids ELSE array_append(report_ids, $4) END
        WHERE target_type = $1::report_target_type AND target_id = $2 AND restriction = $3
          AND lifted_at IS NULL AND superseded_by IS NULL AND $5::uuid IS NULL
        RETURNING id
        "#,
    )
    .bind(d.target_type)
    .bind(d.target_id)
    .bind(d.restriction.as_str())
    .bind(d.report_id)
    .bind(d.rights_claim_id)
    .fetch_optional(&mut *tx)
    .await
    .db_err("Database error")?;
    if repeat.is_some() {
        return Ok(pending);
    }

    let (ground_type, ground, reason_text) = ground_for(d.reason);
    let mut explanation = format!("{} {}", reason_text, restriction_text(d.restriction, d.automated));
    if let Some(detail) = &d.detail {
        explanation = format!("{} {}", explanation, detail);
    }
    let deliver = !held_back(d.reason);

    let decision_id = sqlx::query_scalar::<_, Uuid>(
        r#"
        INSERT INTO moderation_decisions
            (target_type, target_id, affected_user_id, restriction, automated, reason,
             ground_type, ground, explanation, content_excerpt, decided_by, report_ids, delivered_at, rights_claim_id)
        VALUES ($1::report_target_type, $2, $3, $4, $5, $6::report_reason, $7, $8, $9, $10, $11,
                CASE WHEN $12::uuid IS NULL THEN '{}'::uuid[] ELSE ARRAY[$12]::uuid[] END,
                CASE WHEN $13 THEN NOW() END, $14)
        RETURNING id
        "#,
    )
    .bind(d.target_type)
    .bind(d.target_id)
    .bind(affected_user_id)
    .bind(d.restriction.as_str())
    .bind(d.automated)
    .bind(d.reason)
    .bind(ground_type)
    .bind(ground)
    .bind(&explanation)
    .bind(excerpt(text))
    .bind(d.decided_by)
    .bind(d.report_id)
    .bind(deliver)
    .bind(d.rights_claim_id)
    .fetch_one(&mut *tx)
    .await
    .db_err_ctx("Failed to record moderation decision", "Database error")?;

    if d.restriction == Restriction::Removed {
        sqlx::query(
            r#"
            UPDATE moderation_decisions SET superseded_by = $3
            WHERE target_type = $1::report_target_type AND target_id = $2 AND id != $3
              AND lifted_at IS NULL AND superseded_by IS NULL
            "#,
        )
        .bind(d.target_type)
        .bind(d.target_id)
        .bind(decision_id)
        .execute(&mut *tx)
        .await
        .db_err("Database error")?;
    }

    if deliver {
        pending.extend(deliver_notice(tx, decision_id, affected_user_id).await?);
    }
    tracing::info!(
        "Moderation decision {}: {} {} {} ({}){}",
        decision_id, d.restriction.as_str(), d.target_type, d.target_id, d.reason,
        if deliver { "" } else { ", statement held back" }
    );
    Ok(pending)
}

async fn deliver_notice(tx: &mut PgConnection, decision_id: Uuid, user_id: Uuid) -> Result<PendingNotices, AppError> {
    let mut pending = PendingNotices::default();
    pending.events.push(insert_system_notification(tx, user_id, "moderation_decision", Some(decision_id)).await?);
    let email = sqlx::query_scalar::<_, String>("SELECT email FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_optional(&mut *tx)
        .await
        .db_err("Database error")?;
    if let Some(email) = email {
        pending.emails.push((email, decision_id));
    }
    Ok(pending)
}

/// Sends a held-back statement (admin action). Returns false if there is
/// nothing to send: already delivered, or its author's account is gone.
pub async fn release_decision(tx: &mut PgConnection, decision_id: Uuid) -> Result<(bool, PendingNotices), AppError> {
    let user = sqlx::query_scalar::<_, Option<Uuid>>(
        "UPDATE moderation_decisions SET delivered_at = NOW() WHERE id = $1 AND delivered_at IS NULL RETURNING affected_user_id",
    )
    .bind(decision_id)
    .fetch_optional(&mut *tx)
    .await
    .db_err("Database error")?;
    match user {
        Some(Some(user_id)) => Ok((true, deliver_notice(tx, decision_id, user_id).await?)),
        _ => Ok((false, PendingNotices::default())),
    }
}

/// When a report is dismissed: lifts the automatic restrictions on the
/// target and tells the author. A held-back statement is delivered now,
/// since there's nothing left to protect and the author should know the
/// content was temporarily restricted.
pub async fn lift_restrictions(tx: &mut PgConnection, target_type: &str, target_id: Uuid) -> Result<PendingNotices, AppError> {
    let mut pending = PendingNotices::default();
    let lifted = sqlx::query_as::<_, (Uuid, Option<Uuid>)>(
        r#"
        UPDATE moderation_decisions SET lifted_at = NOW(), delivered_at = COALESCE(delivered_at, NOW())
        WHERE target_type = $1::report_target_type AND target_id = $2
          AND lifted_at IS NULL AND superseded_by IS NULL AND restriction != 'removed'
        RETURNING id, affected_user_id
        "#,
    )
    .bind(target_type)
    .bind(target_id)
    .fetch_all(&mut *tx)
    .await
    .db_err("Database error")?;

    for (decision_id, user_id) in lifted {
        if let Some(user_id) = user_id {
            pending.events.push(insert_system_notification(tx, user_id, "moderation_decision", Some(decision_id)).await?);
        }
    }
    Ok(pending)
}

/// Tells a reporter their report was reviewed (Art. 16(5)). The reporter
/// sees the outcome on /moderation; nothing about the reported person is
/// revealed beyond what they already know.
pub async fn notify_report_outcome(tx: &mut PgConnection, report_id: Uuid, admin_id: Uuid) -> Result<PendingNotices, AppError> {
    let mut pending = PendingNotices::default();
    let reporter = sqlx::query_scalar::<_, Option<Uuid>>("SELECT reporter_id FROM reports WHERE id = $1")
        .bind(report_id)
        .fetch_optional(&mut *tx)
        .await
        .db_err("Database error")?
        .flatten();
    // An admin reviewing their own report doesn't need telling.
    if let Some(reporter_id) = reporter.filter(|id| *id != admin_id) {
        pending.events.push(insert_system_notification(tx, reporter_id, "report_outcome", None).await?);
    }
    Ok(pending)
}

/// Tells the author the outcome of their objection.
pub async fn notify_objection_resolved(tx: &mut PgConnection, decision_id: Uuid, user_id: Uuid) -> Result<PendingNotices, AppError> {
    let mut pending = PendingNotices::default();
    pending.events.push(insert_system_notification(tx, user_id, "objection_resolved", Some(decision_id)).await?);
    Ok(pending)
}

/// Before an account is deleted: its decision records stay as the audit
/// trail (affected_user_id becomes NULL), but the content excerpt goes.
pub async fn forget_user(tx: &mut PgConnection, user_id: Uuid) -> Result<(), AppError> {
    sqlx::query("UPDATE moderation_decisions SET content_excerpt = NULL WHERE affected_user_id = $1")
        .bind(user_id)
        .execute(&mut *tx)
        .await
        .db_err_ctx("Failed to clear decision excerpts", "Database error")?;
    Ok(())
}

/// For the data export (Art. 15): statements about this user's content,
/// and the reports they filed.
pub async fn export_for(conn: &mut PgConnection, user_id: Uuid) -> Result<serde_json::Value, AppError> {
    let decisions = sqlx::query_scalar::<_, serde_json::Value>(
        r#"
        SELECT jsonb_build_object('id', id, 'target_type', target_type, 'restriction', restriction,
            'automated', automated, 'reason', reason, 'ground', ground, 'explanation', explanation,
            'content_excerpt', content_excerpt, 'created_at', created_at, 'lifted_at', lifted_at,
            'objection', objection, 'objection_status', objection_status, 'objection_response', objection_response)
        FROM moderation_decisions WHERE affected_user_id = $1 AND delivered_at IS NOT NULL
        ORDER BY created_at
        "#,
    )
    .bind(user_id)
    .fetch_all(&mut *conn)
    .await
    .db_err_ctx("Data export query failed", "Database error")?;

    let reports = sqlx::query_scalar::<_, serde_json::Value>(
        r#"
        SELECT jsonb_build_object('id', id, 'target_type', target_type, 'reason', reason,
            'details', details, 'status', status, 'created_at', created_at, 'reviewed_at', reviewed_at)
        FROM reports WHERE reporter_id = $1 ORDER BY created_at
        "#,
    )
    .bind(user_id)
    .fetch_all(&mut *conn)
    .await
    .db_err_ctx("Data export query failed", "Database error")?;

    // Rights claims filed while signed in (the form also works without an
    // account; those aren't linked to anyone).
    let rights_claims = sqlx::query_scalar::<_, serde_json::Value>(
        r#"
        SELECT jsonb_build_object('id', id, 'claim_type', claim_type, 'claimant_name', claimant_name,
            'claimant_email', claimant_email, 'content_url', content_url, 'work_description', work_description,
            'ownership_basis', ownership_basis, 'status', status, 'created_at', created_at, 'decided_at', decided_at)
        FROM rights_claims WHERE claimant_user_id = $1 ORDER BY created_at
        "#,
    )
    .bind(user_id)
    .fetch_all(&mut *conn)
    .await
    .db_err_ctx("Data export query failed", "Database error")?;

    Ok(json!({ "moderation_decisions": decisions, "reports_filed": reports, "rights_claims_filed": rights_claims }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn excerpt_trims_and_shortens() {
        assert_eq!(excerpt(None), None);
        assert_eq!(excerpt(Some("   ".into())), None);
        assert_eq!(excerpt(Some(" hi ".into())).as_deref(), Some("hi"));
        let long = "ä".repeat(300);
        let e = excerpt(Some(long)).unwrap();
        assert_eq!(e.chars().count(), EXCERPT_CHARS + 1);
        assert!(e.ends_with('…'));
    }

    #[test]
    fn every_reason_has_a_ground() {
        for reason in ["spam", "harassment", "hate_speech", "violence", "self_harm",
                       "sexual_content", "csam", "impersonation", "other", "copyright"] {
            let (ground_type, ground, text) = ground_for(reason);
            assert!(ground_type == "illegal" || ground_type == "terms");
            assert!(!ground.is_empty() && !text.is_empty());
        }
        assert_eq!(ground_for("csam").0, "illegal");
        assert!(held_back("csam") && !held_back("violence"));
    }
}
