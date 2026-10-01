//! Moderation decisions: statements of reasons (DSA Art. 17), report
//! outcomes (Art. 16(5)), and the state of restricted content.
//!
//! Whenever someone's post, comment, message or profile is restricted --
//! removed by an admin, hidden after a rights claim, or automatically hidden
//! or shown behind a warning after a report -- `record_decision` stores a
//! decision record. Delivering it notifies the author in the app and, at a
//! verified address, by email, linking to the statement at
//! /moderation/decisions/:id, where they can also object. Reporters learn
//! what came of their report when `close_reports` closes it.
//!
//! The same item can be restricted for several reasons at once: a hide after
//! a CSAM report, a warning after a violence report, an accepted rights
//! claim, a removal. Each is a decision of its own, and `refresh_status`
//! derives posts.moderation_status / comments.moderation_status from the
//! decisions still in force (removed beats hidden beats flagged beats
//! visible). No workflow writes that column itself, so ending one
//! restriction -- a dismissed report, an accepted objection -- can't undo
//! another one.
//!
//! The statement texts are built here, in one place, and stored exactly as
//! shown. ⚖️ Wording and the reason-to-ground mapping are pending legal review.

use serde_json::json;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::errors::AppError;
use crate::handlers::auth::AppState;
use crate::handlers::notifications::{publish_notification, NotificationEvent, NotificationResponse};
use crate::standing::{self, Classification, Measure, NewStrike};
use crate::utils::DbResultExt;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
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

/// What a decision followed, named in its statement (Art. 17(3)(b)).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    /// A report in the app or a notice from the public form.
    Notice,
    /// The team came across it without a notice.
    OwnInitiative,
    /// An order from an authority (Art. 9 DSA, or a removal order for
    /// terrorist content under Regulation 2021/784).
    AuthorityOrder { authority: String, reference: Option<String> },
    RightsClaim,
}

impl Source {
    pub fn as_db(&self) -> &'static str {
        match self {
            Source::Notice => "notice",
            Source::OwnInitiative => "own_initiative",
            Source::AuthorityOrder { .. } => "authority_order",
            Source::RightsClaim => "rights_claim",
        }
    }
}

/// The source of a decision resting on these reports: an authority's order
/// if one of them is, a notice if any came from someone else, otherwise the
/// team's own initiative.
pub async fn source_of_reports(conn: &mut PgConnection, report_ids: &[Uuid]) -> Result<Source, AppError> {
    let rows = sqlx::query_as::<_, (String, Option<String>, Option<String>)>(
        "SELECT source, authority, order_reference FROM reports WHERE id = ANY($1) ORDER BY created_at",
    )
    .bind(report_ids)
    .fetch_all(&mut *conn)
    .await
    .db_err("Database error")?;
    if let Some((_, authority, reference)) = rows.iter().find(|r| r.0 == "authority_order") {
        return Ok(Source::AuthorityOrder { authority: authority.clone().unwrap_or_default(), reference: reference.clone() });
    }
    if rows.iter().any(|r| r.0 == "user_report" || r.0 == "public_notice") {
        return Ok(Source::Notice);
    }
    Ok(Source::OwnInitiative)
}

/// Statements for CSAM are held back until an admin releases them, so a
/// suspect isn't alerted before the content has been reviewed and, where
/// needed, reported to the authorities. ⚖️ Pending legal review.
fn held_back(reason: &str) -> bool {
    reason == "csam"
}

/// The ground a restriction for this report reason rests on: whether it's
/// a legal provision or our Terms of Service, and which one, plus the
/// sentence a statement opens with when the restriction followed a notice.
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
            "Nutzungsbedingungen Abschnitt 4 (Hassrede und Entmenschlichung; rechtswidrige Inhalte, u. a. Volksverhetzung)",
            "Der Inhalt wurde als Hassrede gemeldet.",
        ),
        "harassment" => (
            "terms",
            "Nutzungsbedingungen Abschnitt 4 (Belästigung, Mobbing, Bedrohung oder Stalking)",
            "Der Inhalt wurde als Belästigung oder Mobbing gemeldet.",
        ),
        "self_harm" => (
            "terms",
            "Nutzungsbedingungen Abschnitt 4 (Ermutigung oder Anleitung zu Selbstverletzung oder Suizid)",
            "Der Inhalt wurde als Darstellung von Selbstverletzung oder Suizid gemeldet.",
        ),
        "sexual_content" => (
            "terms",
            "Nutzungsbedingungen Abschnitt 4 (pornografische Inhalte, sexuelle Belästigung)",
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
        // ⚖️ The four below may also be crimes (e.g. § 263, § 201a, §§ 89a,
        // 126, 129a, § 29 BtMG / § 52 WaffG); like hate speech, the statement
        // cites our terms rather than asserting a crime.
        "fraud" => (
            "terms",
            "Nutzungsbedingungen Abschnitt 4 (Betrug und Betrugsversuche)",
            "Der Inhalt wurde als Betrug oder Betrugsversuch gemeldet.",
        ),
        "ncii" => (
            "terms",
            "Nutzungsbedingungen Abschnitt 4 (intime Aufnahmen ohne Einwilligung)",
            "Der Inhalt wurde als intime Aufnahme gemeldet, die ohne Einwilligung der gezeigten Person verbreitet wird.",
        ),
        "terrorism" => (
            "terms",
            "Nutzungsbedingungen Abschnitt 4 (Terrorismus und Androhung schwerer Gewalt)",
            "Der Inhalt wurde als terroristischer Inhalt oder als Androhung schwerer Gewalt gemeldet.",
        ),
        "extremism" => (
            "terms",
            "Nutzungsbedingungen Abschnitt 4 (Extremismus; Verherrlichung des Nationalsozialismus oder Faschismus)",
            "Der Inhalt wurde als extremistisch oder als Verherrlichung des Nationalsozialismus oder Faschismus gemeldet.",
        ),
        "illegal_goods" => (
            "terms",
            "Nutzungsbedingungen Abschnitt 4 (Handel mit illegalen Waren)",
            "Der Inhalt wurde als Angebot oder Handel mit illegalen Waren, etwa Drogen oder Waffen, gemeldet.",
        ),
        _ => (
            "terms",
            "Nutzungsbedingungen Abschnitt 4",
            "Der Inhalt wurde wegen eines Verstoßes gegen unsere Nutzungsbedingungen gemeldet.",
        ),
    }
}

/// The sentence a statement opens with: what the decision followed.
fn source_text(source: &Source, reason: &str) -> String {
    match source {
        Source::Notice | Source::RightsClaim => ground_for(reason).2.to_string(),
        Source::OwnInitiative => {
            "Unser Team ist bei einer eigenen Prüfung auf den Inhalt gestoßen; die Entscheidung beruht nicht auf einer Meldung."
                .to_string()
        }
        Source::AuthorityOrder { authority, reference } => format!(
            "Wir haben eine Anordnung von {}{} erhalten, gegen den Inhalt vorzugehen. Gegen die Anordnung selbst \
             kannst du dich bei der anordnenden Stelle oder vor Gericht wehren.",
            authority,
            reference.as_deref().map(|r| format!(" (Aktenzeichen {})", r)).unwrap_or_default()
        ),
    }
}

/// The ground cited: an authority's order rests on the law it applies, which
/// the order names; the reason's ground follows as what the content was.
fn ground_with_source(source: &Source, reason: &str) -> (&'static str, String) {
    let (ground_type, ground, _) = ground_for(reason);
    match source {
        Source::AuthorityOrder { authority, .. } => (
            "illegal",
            format!("Behördliche Anordnung ({}, Art. 9 Digital Services Act); {}", authority, ground),
        ),
        _ => (ground_type, ground.to_string()),
    }
}

/// Added to every statement about suicide or self-harm: someone in a crisis
/// shouldn't get only a decision from us.
const CRISIS_HELP: &str = "Falls es dir gerade nicht gut geht: Du bist damit nicht allein. Die TelefonSeelsorge ist rund \
    um die Uhr kostenlos und anonym für dich da, unter 0800 111 0 111, 0800 111 0 222 oder 116 123 und im Chat auf \
    telefonseelsorge.de. In einem akuten Notfall wähle 112.";

/// What happened to the content, as the statement says it.
fn restriction_text(restriction: Restriction, automated: bool, target_type: &str, reason: &str, gone: bool) -> &'static str {
    match restriction {
        Restriction::Removed if gone => {
            "Unser Team hat den Inhalt geprüft und einen Verstoß festgestellt. Du hattest ihn zu diesem Zeitpunkt bereits \
             selbst gelöscht."
        }
        Restriction::Removed if target_type == "message" => {
            "Unser Team hat die Nachricht geprüft und entfernt; sie ist für beide Seiten gelöscht."
        }
        Restriction::Removed if target_type == "user" => {
            "Unser Team hat dein Profil geprüft und die unten genannten Angaben entfernt."
        }
        // Deleted for good as soon as the evidence copy is made (see
        // purge_removed); there's nothing to restore.
        Restriction::Removed if reason == "csam" => "Unser Team hat den Inhalt geprüft und entfernt.",
        Restriction::Removed => {
            "Unser Team hat den Inhalt geprüft und entfernt. Bis zum Ende der Widerspruchsfrist bewahren wir ihn \
             unsichtbar auf: Hat dein Widerspruch Erfolg, stellen wir ihn wieder her, sonst löschen wir ihn danach \
             endgültig."
        }
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

/// The classification part of a removal's statement: which violation type
/// the team found, its criterion, and the points it counts. The admin's
/// justification stays internal.
fn classification_text(c: &Classification, points: Option<(i32, i32)>) -> String {
    match (c.violation, points) {
        (Some(v), Some((base, points))) => format!(
            "Eingestuft als „{}“: {} Dafür werden deinem Konto {} Punkte angerechnet{}.",
            v.label_de,
            v.criterion_de,
            points,
            if points > base {
                format!(
                    " ({} Punkte, eineinhalbfach, weil es mindestens der dritte Verstoß aus diesem Grund innerhalb von {} Tagen ist)",
                    base, standing::REPEAT_WINDOW_DAYS
                )
            } else {
                String::new()
            }
        ),
        (Some(v), None) => format!("Eingestuft als „{}“: {} Dafür werden keine Punkte angerechnet.", v.label_de, v.criterion_de),
        (None, _) => "Dafür werden keine Punkte angerechnet.".to_string(),
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
    /// Outcome emails to people who used the public notice form:
    /// (email, notice id, outcome).
    notifier_emails: Vec<(String, Uuid, String)>,
    alerts: Vec<crate::alerts::Alert>,
}

impl PendingNotices {
    pub fn extend(&mut self, other: PendingNotices) {
        self.events.extend(other.events);
        self.emails.extend(other.emails);
        self.notifier_emails.extend(other.notifier_emails);
        self.alerts.extend(other.alerts);
    }

    pub fn alert(&mut self, alert: crate::alerts::Alert) {
        self.alerts.push(alert);
    }

    /// Pushes the live notifications and sends the emails. Runs after
    /// commit; failures are logged, since the notices are stored and show up
    /// in the app regardless.
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
        for (email, notice_id, outcome) in self.notifier_emails {
            let state = state.clone();
            tokio::spawn(async move {
                if let Err(e) = state.email.send_notice_outcome(&email, notice_id, &outcome).await {
                    tracing::error!("Outcome email for notice {} failed: {}", notice_id, e.0);
                }
            });
        }
        for alert in self.alerts {
            crate::alerts::send(state, alert).await;
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
    /// The reports behind it; empty for a rights claim.
    pub report_ids: Vec<Uuid>,
    pub rights_claim_id: Option<Uuid>,
    pub source: Source,
    /// Appended to the explanation, e.g. which work a rights claim is about.
    pub detail: Option<String>,
    /// For an admin's removal: how they classified the violation, which
    /// sets the author's strike (standing.rs).
    pub classification: Option<Classification>,
    /// For a removal from a profile: what the statement shows as affected,
    /// and what was removed (handlers/profile_moderation.rs).
    pub excerpt: Option<String>,
    pub removed_fields: Option<serde_json::Value>,
}

impl<'a> NewDecision<'a> {
    /// A decision with no reports, no classification and nothing extra,
    /// for struct-update syntax.
    pub fn base(target_type: &'a str, target_id: Uuid, restriction: Restriction, reason: &'a str) -> Self {
        NewDecision {
            target_type,
            target_id,
            restriction,
            automated: false,
            reason,
            decided_by: None,
            report_ids: Vec::new(),
            rights_claim_id: None,
            source: Source::Notice,
            detail: None,
            classification: None,
            excerpt: None,
            removed_fields: None,
        }
    }
}

/// What `record_decision` did.
pub struct Recorded {
    /// None when there was nothing left to decide on: the content is gone
    /// and nothing of it was preserved.
    pub decision_id: Option<Uuid>,
    /// The content had already been deleted; the decision rests on its
    /// preserved copy.
    pub content_gone: bool,
    pub notices: PendingNotices,
}

/// The author and text of the content a decision is about.
struct DecidedContent {
    /// None once the author's account is gone too.
    author_id: Option<Uuid>,
    text: Option<String>,
    /// The content itself is gone (deleted by its author); author and text
    /// come from the evidence copy.
    gone: bool,
}

async fn decided_content(tx: &mut PgConnection, target_type: &str, target_id: Uuid) -> Result<Option<DecidedContent>, AppError> {
    let sql = match target_type {
        "post" => "SELECT user_id, caption FROM posts WHERE id = $1",
        "comment" => "SELECT user_id, body FROM comments WHERE id = $1",
        "message" => "SELECT sender_id, body FROM messages WHERE id = $1",
        "user" => "SELECT id, NULL::text FROM users WHERE id = $1",
        _ => return Ok(None),
    };
    let live = sqlx::query_as::<_, (Option<Uuid>, Option<String>)>(sql)
        .bind(target_id)
        .fetch_optional(&mut *tx)
        .await
        .db_err("Database error")?;
    if let Some((author_id, text)) = live {
        return Ok(Some(DecidedContent { author_id, text, gone: false }));
    }

    // Deleted before anyone decided: a report for a likely-illegal reason
    // preserved it (evidence.rs), so deleting doesn't escape the decision.
    let preserved = sqlx::query_as::<_, (Option<String>, Option<String>)>(
        r#"
        SELECT v.content->'author'->>'id',
               COALESCE(v.content->'post'->>'caption', v.content->'comment'->>'body', v.content->'message'->>'body')
        FROM evidence_versions v JOIN evidence_records e ON e.id = v.evidence_id
        WHERE e.target_type = $1::report_target_type AND e.target_id = $2 AND e.purged_at IS NULL
        ORDER BY v.captured_at DESC, v.id DESC LIMIT 1
        "#,
    )
    .bind(target_type)
    .bind(target_id)
    .fetch_optional(&mut *tx)
    .await
    .db_err("Database error")?;
    let Some((author, text)) = preserved else {
        return Ok(None);
    };
    // The author may have deleted their whole account.
    let author_id = match author.and_then(|a| a.parse::<Uuid>().ok()) {
        Some(id) => sqlx::query_scalar::<_, Uuid>("SELECT id FROM users WHERE id = $1")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await
            .db_err("Database error")?,
        None => None,
    };
    // A deleted account's profile text isn't kept in the record of the
    // decision; the evidence copy has it.
    let text = if target_type == "user" { None } else { text };
    Ok(Some(DecidedContent { author_id, text, gone: true }))
}

/// Records a restriction of a post, comment, message or profile, inside the
/// transaction that imposes it. A repeat of an active automatic restriction
/// (a second report that would flag an already-flagged post) only adds its
/// reports to it. A removal supersedes the automatic restrictions before it;
/// a pending objection to one of those is closed, since the removal can be
/// objected to itself. The caller applies the restriction with
/// `refresh_status` (posts, comments) or by deleting or changing the item.
pub async fn record_decision(tx: &mut PgConnection, d: NewDecision<'_>) -> Result<Recorded, AppError> {
    let mut pending = PendingNotices::default();

    let Some(content) = decided_content(tx, d.target_type, d.target_id).await? else {
        return Ok(Recorded { decision_id: None, content_gone: true, notices: pending });
    };

    if d.automated {
        let repeat = sqlx::query_scalar::<_, Uuid>(
            r#"
            UPDATE moderation_decisions
            SET report_ids = ARRAY(SELECT DISTINCT unnest(report_ids || $4::uuid[]))
            WHERE target_type = $1::report_target_type AND target_id = $2 AND restriction = $3
              AND automated AND lifted_at IS NULL AND superseded_by IS NULL
            RETURNING id
            "#,
        )
        .bind(d.target_type)
        .bind(d.target_id)
        .bind(d.restriction.as_str())
        .bind(&d.report_ids)
        .fetch_optional(&mut *tx)
        .await
        .db_err("Database error")?;
        if let Some(id) = repeat {
            return Ok(Recorded { decision_id: Some(id), content_gone: content.gone, notices: pending });
        }
    }

    let removal = d.restriction == Restriction::Removed;
    // The strike's points are worked out first, so the statement can say
    // exactly what the removal counts.
    let removal_violation = d.classification.as_ref().filter(|_| removal);
    let strike_points = match (removal_violation.and_then(|c| c.violation), content.author_id) {
        (Some(v), Some(author)) if v.severity.points() > 0 => Some(standing::strike_points(tx, author, v).await?),
        _ => None,
    };

    let (ground_type, ground) = ground_with_source(&d.source, d.reason);
    let mut explanation = format!(
        "{} {}",
        source_text(&d.source, d.reason),
        restriction_text(d.restriction, d.automated, d.target_type, d.reason, content.gone)
    );
    if let Some(detail) = &d.detail {
        explanation = format!("{} {}", explanation, detail);
    }
    if let Some(c) = removal_violation {
        explanation = format!("{} {}", explanation, classification_text(c, strike_points));
    }
    let crisis = d.reason == "self_harm" || removal_violation.and_then(|c| c.violation).is_some_and(|v| v.reason == "self_harm");
    if crisis {
        explanation = format!("{} {}", explanation, CRISIS_HELP);
    }

    // Held back: CSAM, and anything else about an item whose statement is
    // still held back, so a second report on it can't tip the author off.
    let target_held = sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS(SELECT 1 FROM moderation_decisions
                      WHERE target_type = $1::report_target_type AND target_id = $2
                        AND delivered_at IS NULL AND lifted_at IS NULL AND superseded_by IS NULL)
        "#,
    )
    .bind(d.target_type)
    .bind(d.target_id)
    .fetch_one(&mut *tx)
    .await
    .db_err("Database error")?;
    let deliver = content.author_id.is_some() && !held_back(d.reason) && !target_held;
    // Nothing is left to delete later: the item was already gone, or a
    // message or profile change takes effect at once -- except a removed
    // profile picture, kept for a possible objection like removed posts
    // (profile_moderation.rs).
    let keeps_file = d.removed_fields.as_ref().is_some_and(|f| f.get("avatar_key").is_some());
    let purged_now = removal && (content.gone || d.target_type == "message" || (d.target_type == "user" && !keeps_file));

    let decision_id = sqlx::query_scalar::<_, Uuid>(
        r#"
        INSERT INTO moderation_decisions
            (target_type, target_id, affected_user_id, restriction, automated, reason,
             ground_type, ground, explanation, content_excerpt, decided_by, report_ids, delivered_at, rights_claim_id,
             violation_type, violation_note, source, removed_fields, content_purged_at)
        VALUES ($1::report_target_type, $2, $3, $4, $5, $6::report_reason, $7, $8, $9, $10, $11, $12,
                CASE WHEN $13 THEN NOW() END, $14, $15, $16, $17, $18, CASE WHEN $19 THEN NOW() END)
        RETURNING id
        "#,
    )
    .bind(d.target_type)
    .bind(d.target_id)
    .bind(content.author_id)
    .bind(d.restriction.as_str())
    .bind(d.automated)
    .bind(d.reason)
    .bind(ground_type)
    .bind(&ground)
    .bind(&explanation)
    .bind(d.excerpt.clone().or_else(|| excerpt(content.text.clone())))
    .bind(d.decided_by)
    .bind(&d.report_ids)
    .bind(deliver)
    .bind(d.rights_claim_id)
    .bind(removal_violation.map(|c| c.id()))
    .bind(removal_violation.and_then(|c| c.note.as_deref()))
    .bind(d.source.as_db())
    .bind(&d.removed_fields)
    .bind(purged_now)
    .fetch_one(&mut *tx)
    .await
    .db_err_ctx("Failed to record moderation decision", "Database error")?;

    if removal {
        if let (Some(v), Some((base_points, points)), Some(author)) =
            (removal_violation.and_then(|c| c.violation), strike_points, content.author_id)
        {
            standing::add_strike(tx, NewStrike {
                user_id: author,
                decision_id,
                violation: v,
                base_points,
                points,
                target_type: d.target_type,
                target_id: d.target_id,
                fallback_text: if content.gone { content.text.clone() } else { None },
                removed_fields: d.removed_fields.clone(),
            })
            .await?;
        }
        pending.extend(supersede_automatic(tx, d.target_type, d.target_id, decision_id, d.decided_by).await?);
    }

    if deliver {
        if let Some(author) = content.author_id {
            pending.extend(deliver_notice(tx, decision_id, author).await?);
        }
    }
    tracing::info!(
        "Moderation decision {}: {} {} {} ({}, {}){}{}",
        decision_id, d.restriction.as_str(), d.target_type, d.target_id, d.reason, d.source.as_db(),
        if content.gone { ", content already deleted" } else { "" },
        if deliver { "" } else { ", statement held back" }
    );
    Ok(Recorded { decision_id: Some(decision_id), content_gone: content.gone, notices: pending })
}

/// A removal replaces the automatic restrictions on the same item. An
/// objection still pending against one of them is closed: the removal says
/// what the team decided, and can be objected to itself.
async fn supersede_automatic(
    tx: &mut PgConnection,
    target_type: &str,
    target_id: Uuid,
    removal_id: Uuid,
    admin_id: Option<Uuid>,
) -> Result<PendingNotices, AppError> {
    let mut pending = PendingNotices::default();
    let superseded = sqlx::query_as::<_, (Uuid, Option<Uuid>, Option<String>)>(
        r#"
        UPDATE moderation_decisions SET superseded_by = $3
        WHERE target_type = $1::report_target_type AND target_id = $2 AND id != $3
          AND automated AND lifted_at IS NULL AND superseded_by IS NULL
        RETURNING id, affected_user_id, objection_status
        "#,
    )
    .bind(target_type)
    .bind(target_id)
    .bind(removal_id)
    .fetch_all(&mut *tx)
    .await
    .db_err("Database error")?;

    for (decision_id, user_id, objection_status) in superseded {
        if objection_status.as_deref() != Some("pending") {
            continue;
        }
        sqlx::query(
            r#"
            UPDATE moderation_decisions
            SET objection_status = 'superseded', objection_resolved_at = NOW(), objection_resolved_by = $2,
                objection_response = 'Unser Team hat den Inhalt inzwischen geprüft und entfernt. Diese Entscheidung ersetzt die automatische Einschränkung, gegen die sich dein Widerspruch richtete. Gegen die Entfernung kannst du in ihrer eigenen Begründung widersprechen.'
            WHERE id = $1 AND objection_status = 'pending'
            "#,
        )
        .bind(decision_id)
        .bind(admin_id)
        .execute(&mut *tx)
        .await
        .db_err("Database error")?;
        if let Some(user_id) = user_id {
            pending.events.push(insert_system_notification(tx, user_id, "objection_resolved", Some(decision_id)).await?);
        }
    }
    Ok(pending)
}

/// Sets a post's or comment's moderation_status from the decisions still in
/// force on it -- removed beats hidden beats flagged beats visible -- and
/// keeps the counters that leave out removed content (the author's
/// post_count, the post's comment_count) in step. Returns the status before
/// and after; None for other targets or when the item no longer exists.
pub async fn refresh_status(tx: &mut PgConnection, target_type: &str, target_id: Uuid) -> Result<Option<(String, String)>, AppError> {
    let table = match target_type {
        "post" => "posts",
        "comment" => "comments",
        _ => return Ok(None),
    };
    let Some(old) = sqlx::query_scalar::<_, String>(&format!(
        "SELECT moderation_status::text FROM {table} WHERE id = $1 FOR UPDATE"
    ))
    .bind(target_id)
    .fetch_optional(&mut *tx)
    .await
    .db_err("Database error")?
    else {
        return Ok(None);
    };

    let new = sqlx::query_scalar::<_, String>(
        r#"
        SELECT CASE
            WHEN bool_or(restriction = 'removed') THEN 'removed'
            WHEN bool_or(restriction = 'hidden') THEN 'hidden'
            WHEN bool_or(restriction = 'flagged') THEN 'flagged'
            ELSE 'visible' END
        FROM moderation_decisions
        WHERE target_type = $1::report_target_type AND target_id = $2 AND lifted_at IS NULL AND superseded_by IS NULL
        "#,
    )
    .bind(target_type)
    .bind(target_id)
    .fetch_one(&mut *tx)
    .await
    .db_err("Database error")?;

    if new != old {
        sqlx::query(&format!("UPDATE {table} SET moderation_status = $2::moderation_status WHERE id = $1"))
            .bind(target_id)
            .bind(&new)
            .execute(&mut *tx)
            .await
            .db_err("Database error")?;

        if (old == "removed") != (new == "removed") {
            let delta: i64 = if new == "removed" { -1 } else { 1 };
            let sql = if target_type == "post" {
                "UPDATE users SET post_count = GREATEST(post_count + $2, 0) WHERE id = (SELECT user_id FROM posts WHERE id = $1)"
            } else {
                "UPDATE posts SET comment_count = GREATEST(comment_count + $2, 0) WHERE id = (SELECT post_id FROM comments WHERE id = $1)"
            };
            sqlx::query(sql)
                .bind(target_id)
                .bind(delta)
                .execute(&mut *tx)
                .await
                .db_err_ctx("Failed to update a counter", "Database error")?;
        }
    }
    Ok(Some((old, new)))
}

/// After reports were dismissed: lifts the automatic restrictions that
/// rested on them and on no report still pending, and tells their authors.
/// A pending objection against one of them is answered as accepted -- the
/// review agreed with it -- with the admin's `response`, or a standard
/// answer. Restrictions from a rights claim or the team are left alone; so
/// are those that another pending report still holds up.
pub async fn lift_for_dismissed(
    tx: &mut PgConnection,
    target_type: &str,
    target_id: Uuid,
    dismissed: &[Uuid],
    admin_id: Uuid,
    response: Option<&str>,
) -> Result<PendingNotices, AppError> {
    let mut pending = PendingNotices::default();
    let lifted = sqlx::query_as::<_, (Uuid, Option<Uuid>, Option<String>)>(
        r#"
        UPDATE moderation_decisions d SET lifted_at = NOW(), delivered_at = COALESCE(delivered_at, NOW())
        WHERE d.target_type = $1::report_target_type AND d.target_id = $2
          AND d.automated AND d.lifted_at IS NULL AND d.superseded_by IS NULL
          AND d.report_ids && $3::uuid[]
          AND NOT EXISTS (SELECT 1 FROM reports r WHERE r.id = ANY(d.report_ids) AND r.status = 'pending')
        RETURNING d.id, d.affected_user_id, d.objection_status
        "#,
    )
    .bind(target_type)
    .bind(target_id)
    .bind(dismissed)
    .fetch_all(&mut *tx)
    .await
    .db_err("Database error")?;

    let answer = response.map(str::trim).filter(|r| !r.is_empty()).unwrap_or(
        "Unser Team hat die Meldung geprüft und keinen Verstoß festgestellt. Die Einschränkung ist aufgehoben.",
    );
    for (decision_id, user_id, objection_status) in lifted {
        let objected = objection_status.as_deref() == Some("pending");
        if objected {
            sqlx::query(
                r#"
                UPDATE moderation_decisions
                SET objection_status = 'accepted', objection_response = $2, objection_resolved_at = NOW(), objection_resolved_by = $3
                WHERE id = $1 AND objection_status = 'pending'
                "#,
            )
            .bind(decision_id)
            .bind(answer)
            .bind(admin_id)
            .execute(&mut *tx)
            .await
            .db_err("Database error")?;
        }
        if let Some(user_id) = user_id {
            let kind = if objected { "objection_resolved" } else { "moderation_decision" };
            pending.events.push(insert_system_notification(tx, user_id, kind, Some(decision_id)).await?);
        }
    }
    Ok(pending)
}

/// Closes pending reports with an outcome and tells each reporter (Art.
/// 16(5)): in the app, or by email for a notice from the public form. The
/// reporter learns the outcome, nothing about the reported person beyond
/// what they already know. An admin closing their own report isn't told.
pub async fn close_reports(
    tx: &mut PgConnection,
    report_ids: &[Uuid],
    status: &str,
    outcome: &str,
    admin_id: Option<Uuid>,
    note: Option<&str>,
) -> Result<PendingNotices, AppError> {
    let mut pending = PendingNotices::default();
    let closed = sqlx::query_as::<_, (Uuid, Option<Uuid>, Option<Uuid>)>(
        r#"
        UPDATE reports
        SET status = $2::report_status, outcome = $3, reviewed_at = NOW(), reviewed_by = $4,
            review_note = COALESCE($5, review_note)
        WHERE id = ANY($1) AND status = 'pending'
        RETURNING id, reporter_id, notice_id
        "#,
    )
    .bind(report_ids)
    .bind(status)
    .bind(outcome)
    .bind(admin_id)
    .bind(note)
    .fetch_all(&mut *tx)
    .await
    .db_err_ctx("Failed to close reports", "Database error")?;

    for (_, reporter_id, notice_id) in closed {
        if let Some(reporter_id) = reporter_id.filter(|id| Some(*id) != admin_id) {
            pending.events.push(insert_system_notification(tx, reporter_id, "report_outcome", None).await?);
        }
        if let Some(notice_id) = notice_id {
            let email = sqlx::query_scalar::<_, Option<String>>(
                "UPDATE content_notices SET decided_at = NOW(), outcome = $2 WHERE id = $1 RETURNING notifier_email",
            )
            .bind(notice_id)
            .bind(outcome)
            .fetch_optional(&mut *tx)
            .await
            .db_err("Database error")?
            .flatten();
            if let Some(email) = email {
                pending.notifier_emails.push((email, notice_id, outcome.to_string()));
            }
        }
    }
    Ok(pending)
}

/// Before content is deleted by its author (or with the account): reports
/// on it that nobody needs to decide any more are closed as obsolete, and
/// their reporters learn the content is gone. Reports that preserved the
/// content as evidence (likely-illegal reasons, every report on a message)
/// stay pending: the decision still matters, and the evidence copy is what
/// the admin decides on.
pub async fn close_obsolete_reports(
    tx: &mut PgConnection,
    target_type: &str,
    target_ids: &[Uuid],
) -> Result<PendingNotices, AppError> {
    if target_ids.is_empty() {
        return Ok(PendingNotices::default());
    }
    let ids = sqlx::query_scalar::<_, Uuid>(
        r#"
        SELECT r.id FROM reports r
        WHERE r.status = 'pending' AND r.target_type = $1::report_target_type AND r.target_id = ANY($2)
          AND NOT EXISTS (
              SELECT 1 FROM reports p
              WHERE p.target_type = r.target_type AND p.target_id = r.target_id AND p.status = 'pending'
                AND (p.target_type = 'message' OR p.reason::text = ANY($3))
          )
        "#,
    )
    .bind(target_type)
    .bind(target_ids)
    .bind(crate::evidence::LIKELY_ILLEGAL_REASONS)
    .fetch_all(&mut *tx)
    .await
    .db_err("Database error")?;
    close_reports(tx, &ids, "obsolete", "obsolete", None, None).await
}

/// Why an account measure was taken, in the statement: the admin's
/// explanation, the strikes behind it if there are any, and whether a
/// report led to it. Only an account that has strikes is told about its
/// score.
fn measure_text(measure: Measure, score: i32, has_strikes: bool, basis: Option<&str>, from_report: bool) -> String {
    let mut parts: Vec<String> = Vec::new();
    if from_report {
        parts.push("Die Entscheidung folgt auf eine Meldung deines Kontos.".to_string());
    }
    if let Some(basis) = basis.map(str::trim).filter(|b| !b.is_empty()) {
        parts.push(basis.to_string());
    }
    if has_strikes {
        parts.push(format!(
            "Nach Prüfung wurden wiederholt oder schwerwiegend Inhalte von dir entfernt, weil sie gegen \
             unsere Nutzungsbedingungen verstoßen. Jeder bestätigte Verstoß ergibt je nach Schwere \
             Punkte, die nach einer gewissen Zeit wieder verfallen; dein Konto hat derzeit {} von {} \
             Punkten. Die einzelnen Entscheidungen findest du unter „Kontostatus“.",
            score,
            standing::MAX_SCORE
        ));
    }
    let effect = match measure {
        Measure::Warning => "Wir verwarnen dein Konto. Weitere Verstöße können zu einer vorübergehenden \
             oder dauerhaften Sperrung führen."
            .to_string(),
        Measure::Suspend7 | Measure::Suspend30 => format!(
            "Dein Konto ist für {} Tage gesperrt. Solange kannst du Klar nur lesen: Du kannst nichts \
             veröffentlichen, kommentieren, liken, folgen oder Nachrichten senden, und dein Profil und \
             deine Inhalte sind für andere nicht sichtbar. Deine Daten exportieren, dein Konto löschen \
             und dieser Entscheidung widersprechen kannst du weiterhin.",
            measure.suspension_days().unwrap_or_default()
        ),
        Measure::Ban => format!(
            "Dein Konto ist dauerhaft gesperrt. Du kannst Klar nur noch lesen, und dein Profil und deine \
             Inhalte sind für andere nicht sichtbar. Deine Daten exportieren, dein Konto löschen und dieser \
             Entscheidung widersprechen kannst du weiterhin. Nach Ablauf der Widerspruchsfrist von {} Tagen \
             löschen wir dein Konto mit allen Inhalten, solange kein Widerspruch offen ist; zwei Wochen \
             vorher erinnern wir dich per E-Mail.",
            standing::BAN_DELETION_DAYS
        ),
    };
    parts.push(effect);
    parts.join(" ")
}

/// An account measure decided by an admin.
pub struct NewMeasure<'a> {
    pub user_id: Uuid,
    pub measure: Measure,
    pub reason: &'a str,
    pub decided_by: Uuid,
    pub score: i32,
    pub has_strikes: bool,
    /// Why, in the admin's words or a fixed text (e.g. an account review
    /// that found a bot); shown in the statement.
    pub basis: Option<&'a str>,
    /// Reports on the account that led to it. They are closed with the
    /// outcome "account measure" by the caller.
    pub report_ids: Vec<Uuid>,
}

/// Records an account measure (warning or suspension) decided by an admin,
/// and applies a suspension. A new suspension replaces the current one.
/// Returns the decision id and the notices to send after commit.
pub async fn record_account_measure(tx: &mut PgConnection, m: NewMeasure<'_>) -> Result<(Uuid, PendingNotices), AppError> {
    let (ground_type, ground, _) = ground_for(m.reason);
    let ground = format!("{}; Nutzungsbedingungen Abschnitt 8 (Sperrung von Konten)", ground);
    // ⚖️ Held back like the content decision for CSAM, so a suspect isn't
    // alerted before a report to the authorities. The suspension itself
    // still applies. Pending legal review.
    let deliver = !held_back(m.reason);
    let source = if m.report_ids.is_empty() { None } else { Some("notice") };

    let decision_id = sqlx::query_scalar::<_, Uuid>(
        r#"
        INSERT INTO moderation_decisions
            (target_type, target_id, affected_user_id, restriction, automated, reason, ground_type, ground,
             explanation, decided_by, delivered_at, suspension_days, standing_score, report_ids, source)
        VALUES ('user', $1, $1, $2, FALSE, $3::report_reason, $4, $5, $6, $7, CASE WHEN $8 THEN NOW() END, $9, $10,
                $11, $12)
        RETURNING id
        "#,
    )
    .bind(m.user_id)
    .bind(m.measure.restriction())
    .bind(m.reason)
    .bind(ground_type)
    .bind(&ground)
    .bind(measure_text(m.measure, m.score, m.has_strikes, m.basis, !m.report_ids.is_empty()))
    .bind(m.decided_by)
    .bind(deliver)
    .bind(m.measure.suspension_days())
    .bind(m.score)
    .bind(&m.report_ids)
    .bind(source)
    .fetch_one(&mut *tx)
    .await
    .db_err_ctx("Failed to record account measure", "Database error")?;

    if m.measure != Measure::Warning {
        sqlx::query(
            r#"
            UPDATE moderation_decisions SET superseded_by = $2
            WHERE target_type = 'user' AND target_id = $1 AND id != $2
              AND restriction IN ('suspended', 'banned') AND lifted_at IS NULL AND superseded_by IS NULL
            "#,
        )
        .bind(m.user_id)
        .bind(decision_id)
        .execute(&mut *tx)
        .await
        .db_err("Database error")?;

        sqlx::query(
            r#"
            UPDATE users SET suspension_decision_id = $2,
                suspended_until = CASE WHEN $3::int IS NULL THEN 'infinity'::timestamptz
                                       ELSE NOW() + make_interval(days => $3) END
            WHERE id = $1
            "#,
        )
        .bind(m.user_id)
        .bind(decision_id)
        .bind(m.measure.suspension_days())
        .execute(&mut *tx)
        .await
        .db_err_ctx("Failed to suspend account", "Database error")?;
    }

    let notices = if deliver { deliver_notice(tx, decision_id, m.user_id).await? } else { PendingNotices::default() };
    tracing::info!(
        "Account measure {}: {} for user {} (score {}, by admin {}){}",
        decision_id, m.measure.as_str(), m.user_id, m.score, m.decided_by,
        if deliver { "" } else { ", statement held back" }
    );
    Ok((decision_id, notices))
}

/// Ends the suspension a decision imposed, if it is still the current one.
/// Returns whether a suspension ended.
pub async fn end_suspension(tx: &mut PgConnection, decision_id: Uuid) -> Result<bool, AppError> {
    let ended = sqlx::query(
        "UPDATE users SET suspended_until = NULL, suspension_decision_id = NULL WHERE suspension_decision_id = $1",
    )
    .bind(decision_id)
    .execute(&mut *tx)
    .await
    .db_err("Database error")?
    .rows_affected();
    Ok(ended > 0)
}

/// An admin lifts a user's current suspension early. Returns the notices,
/// or None if the user isn't suspended.
pub async fn lift_suspension(
    tx: &mut PgConnection,
    user_id: Uuid,
    admin_id: Uuid,
) -> Result<Option<PendingNotices>, AppError> {
    let decision = sqlx::query_scalar::<_, Uuid>(
        "SELECT suspension_decision_id FROM users WHERE id = $1 AND suspended_until > NOW() AND suspension_decision_id IS NOT NULL FOR UPDATE",
    )
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await
    .db_err("Database error")?;
    let Some(decision_id) = decision else {
        return Ok(None);
    };

    sqlx::query(
        "UPDATE moderation_decisions SET lifted_at = NOW(), lifted_by = $2, delivered_at = COALESCE(delivered_at, NOW()) WHERE id = $1",
    )
    .bind(decision_id)
    .bind(admin_id)
    .execute(&mut *tx)
    .await
    .db_err("Database error")?;
    end_suspension(tx, decision_id).await?;

    let mut pending = PendingNotices::default();
    pending.events.push(insert_system_notification(tx, user_id, "moderation_decision", Some(decision_id)).await?);
    tracing::info!("Suspension {} of user {} lifted by admin {}", decision_id, user_id, admin_id);
    Ok(Some(pending))
}

/// Notifies the affected user of a statement in the app, and by email when
/// their address is verified: an unverified one may be a typo, i.e. a
/// stranger's inbox, and the statement is in the app either way.
async fn deliver_notice(tx: &mut PgConnection, decision_id: Uuid, user_id: Uuid) -> Result<PendingNotices, AppError> {
    let mut pending = PendingNotices::default();
    pending.events.push(insert_system_notification(tx, user_id, "moderation_decision", Some(decision_id)).await?);
    let email = sqlx::query_scalar::<_, String>("SELECT email FROM users WHERE id = $1 AND email_verified")
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

/// Tells the author the outcome of their objection.
pub async fn notify_objection_resolved(tx: &mut PgConnection, decision_id: Uuid, user_id: Uuid) -> Result<PendingNotices, AppError> {
    let mut pending = PendingNotices::default();
    pending.events.push(insert_system_notification(tx, user_id, "objection_resolved", Some(decision_id)).await?);
    Ok(pending)
}

/// Before an account is deleted: its decision records stay as the audit
/// trail (affected_user_id becomes NULL), but the content excerpt and the
/// removed profile texts go. Objections still pending are withdrawn, so
/// they don't wait in the queue forever for an answer nobody can read.
pub async fn forget_user(tx: &mut PgConnection, user_id: Uuid) -> Result<(), AppError> {
    sqlx::query(
        r#"
        UPDATE moderation_decisions
        SET content_excerpt = NULL, removed_fields = NULL,
            objection_status = CASE WHEN objection_status = 'pending' THEN 'withdrawn' ELSE objection_status END,
            objection_resolved_at = CASE WHEN objection_status = 'pending' THEN NOW() ELSE objection_resolved_at END
        WHERE affected_user_id = $1
        "#,
    )
    .bind(user_id)
    .execute(&mut *tx)
    .await
    .db_err_ctx("Failed to clear decision excerpts", "Database error")?;
    Ok(())
}

/// For the data export (Art. 15): statements about this user's content,
/// and the reports and notices they filed.
pub async fn export_for(conn: &mut PgConnection, user_id: Uuid) -> Result<serde_json::Value, AppError> {
    let decisions = sqlx::query_scalar::<_, serde_json::Value>(
        r#"
        SELECT jsonb_build_object('id', id, 'target_type', target_type, 'restriction', restriction,
            'automated', automated, 'reason', reason, 'ground', ground, 'explanation', explanation,
            'content_excerpt', content_excerpt, 'suspension_days', suspension_days,
            'created_at', created_at, 'lifted_at', lifted_at,
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
            'details', details, 'status', status, 'outcome', outcome, 'created_at', created_at,
            'reviewed_at', reviewed_at, 'recheck_requested_at', recheck_requested_at, 'recheck_note', recheck_note)
        FROM reports WHERE reporter_id = $1 AND source = 'user_report' ORDER BY created_at
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

    // Notices about illegal content sent through the public form while
    // signed in.
    let notices = sqlx::query_scalar::<_, serde_json::Value>(
        r#"
        SELECT jsonb_build_object('id', id, 'reason', reason, 'explanation', explanation, 'content_url', content_url,
            'notifier_name', notifier_name, 'notifier_email', notifier_email, 'created_at', created_at,
            'decided_at', decided_at, 'outcome', outcome)
        FROM content_notices WHERE notifier_user_id = $1 ORDER BY created_at
        "#,
    )
    .bind(user_id)
    .fetch_all(&mut *conn)
    .await
    .db_err_ctx("Data export query failed", "Database error")?;

    // Active strikes behind the account's standing; held-back ones stay
    // out until their statement is released, like the decisions above.
    let strikes = sqlx::query_scalar::<_, serde_json::Value>(
        r#"
        SELECT jsonb_build_object('decision_id', s.decision_id, 'violation', s.violation, 'severity', s.severity,
            'points', s.points, 'created_at', s.created_at, 'expires_at', s.expires_at,
            -- Only the removed content itself: the snapshot's context and
            -- reports are other people's data (Art. 15(4) GDPR).
            'removed_content', s.snapshot->'content')
        FROM account_strikes s JOIN moderation_decisions d ON d.id = s.decision_id
        WHERE s.user_id = $1 AND d.delivered_at IS NOT NULL AND (s.expires_at IS NULL OR s.expires_at > NOW())
        ORDER BY s.created_at
        "#,
    )
    .bind(user_id)
    .fetch_all(&mut *conn)
    .await
    .db_err_ctx("Data export query failed", "Database error")?;

    Ok(json!({
        "moderation_decisions": decisions,
        "account_strikes": strikes,
        "reports_filed": reports,
        "notices_filed": notices,
        "rights_claims_filed": rights_claims,
    }))
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
                       "sexual_content", "csam", "impersonation", "other", "copyright",
                       "fraud", "ncii", "terrorism", "illegal_goods", "extremism"] {
            let (ground_type, ground, text) = ground_for(reason);
            assert!(ground_type == "illegal" || ground_type == "terms");
            assert!(!ground.is_empty() && !text.is_empty());
        }
        assert_eq!(ground_for("csam").0, "illegal");
        assert!(held_back("csam") && !held_back("violence"));
    }

    #[test]
    fn statements_name_their_source() {
        assert!(source_text(&Source::Notice, "spam").contains("gemeldet"));
        assert!(source_text(&Source::OwnInitiative, "spam").contains("nicht auf einer Meldung"));
        let order = Source::AuthorityOrder { authority: "BKA".into(), reference: Some("ST-1".into()) };
        assert!(source_text(&order, "terrorism").contains("Anordnung von BKA (Aktenzeichen ST-1)"));
        assert_eq!(ground_with_source(&order, "terrorism").0, "illegal");
    }

    #[test]
    fn measures_only_cite_a_score_when_there_are_strikes() {
        let without = measure_text(Measure::Warning, 0, false, Some("Gibt sich als jemand anderes aus."), true);
        assert!(!without.contains("Punkten"), "{without}");
        assert!(without.contains("Gibt sich als jemand anderes aus."));
        assert!(without.contains("Meldung deines Kontos"));
        let with = measure_text(Measure::Suspend7, 68, true, None, false);
        assert!(with.contains("68 von 100"));
    }
}
