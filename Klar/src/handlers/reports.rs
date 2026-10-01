//! Content reporting & moderation.
//!
//! POST /reports is available to any authenticated user. Everything else
//! here (the review queue, dismiss, remove) is gated to a small set of
//! admin/moderator accounts via the ADMIN_EMAILS env var -- a
//! comma-separated list of emails, compared case-insensitively (see
//! utils::is_admin_email, shared with GET /users/me's is_admin flag)
//! rather than a full roles system, proportionate to a solo-dev,
//! pre-launch scale. Worth revisiting once moderation needs
//! finer-grained permissions than "can do everything or nothing."
//!
//! JWTs only carry user_id (see auth.rs's Claims -- deliberately no
//! email, so a token doesn't go stale if someone changes their address),
//! so checking against an email allowlist means one extra lookup per
//! admin-gated request to resolve user_id -> email. Cheap, and these
//! endpoints are low-frequency (moderation actions, not hot-path reads).
//!
//! require_admin also requires email_verified, not just an email-string
//! match against ADMIN_EMAILS -- without that, adding an address to
//! ADMIN_EMAILS before its real owner has registered would let *anyone*
//! who registers with that exact address first get admin instantly, with
//! no proof they actually control that inbox. Always register + verify
//! the real admin's account before adding it to ADMIN_EMAILS, never the
//! other way around -- email_verified is defense in depth for that
//! ordering mistake, not a substitute for it (email is UNIQUE, so
//! whoever registers it first keeps it either way).

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::alerts::{self, Alert};
use crate::auth::AuthUser;
use crate::errors::AppError;
use crate::evidence;
use crate::handlers::auth::AppState;
use crate::moderation::{self, NewDecision, PendingNotices, Restriction};
use crate::models::{CreateReportRequest, ReportRow};
use crate::standing::{self, AuthorityReport};
use crate::utils::{delete_media, is_admin_email, DbResultExt, ResolveMedia};

pub const VALID_REASONS: &[&str] = &[
    "spam", "harassment", "hate_speech", "violence",
    "self_harm", "sexual_content", "csam", "impersonation", "other",
    "fraud", "ncii", "terrorism", "illegal_goods", "extremism",
];
const VALID_TARGET_TYPES: &[&str] = &["post", "comment", "user", "message"];

/// CSAM gets zero tolerance: a single report hides the content
/// immediately, with no "view anyway" interstitial and no waiting for a
/// second corroborating report. Non-consensual intimate images too: every
/// further view repeats the harm to the person shown, which outweighs the
/// risk of a false report hiding a post until review.
fn is_critical(reason: &str) -> bool {
    matches!(reason, "csam" | "ncii")
}

/// Serious enough to warn readers, not serious enough to let one report
/// unilaterally remove someone's content outright -- report pile-ons are
/// a real abuse vector, so these get an interstitial ("may violate our
/// guidelines, pending review") rather than outright hiding.
fn is_high_severity(reason: &str) -> bool {
    matches!(reason, "violence" | "self_harm" | "sexual_content" | "terrorism")
}

/// How many reports one account can file a day, and how many of them for
/// the reasons that hide content at once. Generous for anyone reporting
/// what they come across; a wall for someone trying to bury another
/// person's posts in reports.
const DAILY_REPORTS: i64 = 30;
const DAILY_CRITICAL_REPORTS: i64 = 5;

/// A report only restricts content before review (a hide or a warning)
/// when its reporter's account is verified and at least a day old, and
/// fewer than three of its CSAM / intimate-image reports were dismissed in
/// the last 30 days. Otherwise one report from a throwaway account could
/// hide any post and move its files. Such reports still go to the top of
/// the queue, and urgent ones alert the admins at once (alerts.rs).
/// ⚖️ Weighed against the harm of CSAM staying visible until review;
/// pending legal review.
const TRUSTED_AFTER_HOURS: i32 = 24;
const DISMISSED_CRITICAL_LIMIT: i64 = 3;

pub async fn require_admin(db: &sqlx::PgPool, auth: &AuthUser) -> Result<(), AppError> {
    let row = sqlx::query_as::<_, (String, bool)>(
        "SELECT email, email_verified FROM users WHERE id = $1"
    )
    .bind(auth.user_id)
    .fetch_optional(db)
    .await
    .db_err("Database error")?
    .ok_or_else(|| AppError::forbidden("Admin access required"))?;

    let (user_email, email_verified) = row;

    // email_verified is required here, not just an email match -- see
    // this module's doc comment for why (unverified + address-match alone
    // would let anyone who registers a listed address first get admin,
    // with no proof they actually control that inbox).
    if email_verified && is_admin_email(&user_email) {
        Ok(())
    } else {
        Err(AppError::forbidden("Admin access required"))
    }
}

/// The author of an item the reporter may report: a post or comment they
/// can see (the account is public, theirs or followed by them), a message
/// in one of their own conversations, or any account. Content removed by
/// moderation can't be reported again. The same 404 for "doesn't exist" and
/// "not yours to see", so ids can't be probed.
async fn reportable_author(
    conn: &mut PgConnection,
    reporter_id: Uuid,
    target_type: &str,
    target_id: Uuid,
) -> Result<Uuid, AppError> {
    let sql = match target_type {
        "post" => r#"
            SELECT p.user_id,
                   (NOT u.is_private OR u.id = $2
                    OR EXISTS(SELECT 1 FROM follows f WHERE f.follower_id = $2 AND f.following_id = u.id))
            FROM posts p JOIN users u ON u.id = p.user_id
            WHERE p.id = $1 AND p.moderation_status != 'removed'
            "#,
        "comment" => r#"
            SELECT c.user_id,
                   (NOT u.is_private OR u.id = $2
                    OR EXISTS(SELECT 1 FROM follows f WHERE f.follower_id = $2 AND f.following_id = u.id))
            FROM comments c JOIN posts p ON p.id = c.post_id JOIN users u ON u.id = p.user_id
            WHERE c.id = $1 AND c.moderation_status != 'removed' AND p.moderation_status != 'removed'
            "#,
        "message" => r#"
            SELECT m.sender_id, (c.user1_id = $2 OR c.user2_id = $2)
            FROM messages m JOIN conversations c ON c.id = m.conversation_id
            WHERE m.id = $1
            "#,
        _ => "SELECT id, TRUE FROM users WHERE id = $1 AND $2::uuid IS NOT NULL",
    };
    let row = sqlx::query_as::<_, (Option<Uuid>, bool)>(sql)
        .bind(target_id)
        .bind(reporter_id)
        .fetch_optional(&mut *conn)
        .await
        .db_err("Database error")?;
    match row {
        Some((Some(author), true)) => Ok(author),
        _ => Err(AppError::not_found(match target_type {
            "post" => "Post not found",
            "comment" => "Comment not found",
            "message" => "Message not found",
            _ => "User not found",
        })),
    }
}

/// Whether this account's reports may restrict content before review; see
/// TRUSTED_AFTER_HOURS.
async fn trusted_reporter(conn: &mut PgConnection, user_id: Uuid) -> Result<bool, AppError> {
    sqlx::query_scalar::<_, bool>(
        r#"
        SELECT u.email_verified AND u.created_at <= NOW() - make_interval(hours => $2)
           AND (SELECT COUNT(*) FROM reports r
                WHERE r.reporter_id = u.id AND r.reason IN ('csam', 'ncii') AND r.status = 'dismissed'
                  AND r.outcome = 'no_violation' AND r.reviewed_at > NOW() - INTERVAL '30 days') < $3
        FROM users u WHERE u.id = $1
        "#,
    )
    .bind(user_id)
    .bind(TRUSTED_AFTER_HOURS)
    .bind(DISMISSED_CRITICAL_LIMIT)
    .fetch_optional(&mut *conn)
    .await
    .db_err("Database error")
    .map(|trusted| trusted.unwrap_or(false))
}

/// POST /reports (auth required)
pub async fn create_report(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<CreateReportRequest>,
) -> Result<(StatusCode, Json<ReportRow>), AppError> {
    if !VALID_TARGET_TYPES.contains(&input.target_type.as_str()) {
        return Err(AppError::bad_request("Invalid target_type"));
    }
    if !VALID_REASONS.contains(&input.reason.as_str()) {
        return Err(AppError::bad_request("Invalid reason"));
    }
    if let Some(details) = &input.details {
        if details.chars().count() > 1000 {
            return Err(AppError::bad_request("Details must be under 1000 characters"));
        }
    }

    // One transaction: the report, the auto-moderation and the evidence
    // snapshot of the reported state commit together.
    let mut tx = state.db.begin().await.db_err_ctx("Failed to start transaction", "Database error")?;

    let author = reportable_author(&mut tx, auth.user_id, &input.target_type, input.target_id).await?;
    if author == auth.user_id {
        return Err(AppError::bad_request(match input.target_type.as_str() {
            "user" => "You can't report yourself",
            "message" => "You can't report your own message",
            _ => "You can't report your own content",
        }));
    }

    let (today, critical_today) = sqlx::query_as::<_, (i64, i64)>(
        r#"
        SELECT COUNT(*), COUNT(*) FILTER (WHERE reason IN ('csam', 'ncii'))
        FROM reports WHERE reporter_id = $1 AND source = 'user_report' AND created_at > NOW() - INTERVAL '1 day'
        "#,
    )
    .bind(auth.user_id)
    .fetch_one(&mut *tx)
    .await
    .db_err("Database error")?;
    if today >= DAILY_REPORTS || (is_critical(&input.reason) && critical_today >= DAILY_CRITICAL_REPORTS) {
        return Err(AppError {
            status: StatusCode::TOO_MANY_REQUESTS,
            message: "You've reached today's limit for reports. If something can't wait, write to kontakt@klarsocial.eu."
                .into(),
        });
    }

    let report = sqlx::query_as::<_, ReportRow>(
        r#"
        INSERT INTO reports (reporter_id, target_type, target_id, reason, details)
        VALUES ($1, $2::report_target_type, $3, $4::report_reason, $5)
        RETURNING id, reporter_id, target_type::text, target_id, reason::text, details, status::text, created_at
        "#
    )
    .bind(auth.user_id)
    .bind(&input.target_type)
    .bind(input.target_id)
    .bind(&input.reason)
    .bind(input.details.as_deref().map(str::trim).filter(|d| !d.is_empty()))
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| {
        // One pending report per person and item (idx_reports_pending_once).
        if e.as_database_error().and_then(|d| d.constraint()) == Some("idx_reports_pending_once") {
            AppError::conflict("You already reported this. It's waiting for review.")
        } else {
            tracing::error!("Failed to submit report: {}", e);
            AppError::internal("Failed to submit report")
        }
    })?;

    // Auto-moderation: only posts and comments can be hidden or put behind
    // a warning before review. A message or a profile just queues, at the
    // priority its reason implies.
    let mut notices = PendingNotices::default();
    let mut newly_hidden = false;
    let automatic = match input.target_type.as_str() {
        "post" | "comment" if is_critical(&input.reason) => Some(Restriction::Hidden),
        "post" | "comment" if is_high_severity(&input.reason) => Some(Restriction::Flagged),
        _ => None,
    };
    if let Some(restriction) = automatic {
        if trusted_reporter(&mut tx, auth.user_id).await? {
            // An automatic restriction is a decision like any other: its
            // author gets a statement of reasons (DSA Art. 17), marked as
            // automated.
            let recorded = moderation::record_decision(&mut tx, NewDecision {
                automated: true,
                report_ids: vec![report.id],
                ..NewDecision::base(&input.target_type, input.target_id, restriction, &input.reason)
            })
            .await?;
            notices.extend(recorded.notices);
            if let Some((old, new)) = moderation::refresh_status(&mut tx, &input.target_type, input.target_id).await? {
                // Only the change to hidden moves the media keys, so a
                // second report on an already-hidden post doesn't move the
                // files again.
                newly_hidden = input.target_type == "post" && new == "hidden" && old != "hidden";
            }
        }
    }

    // Likely-illegal content (and every reported message) is captured as
    // reported, right away, so later edits or deletion can't destroy the
    // evidence; see evidence.rs.
    let preserved = if evidence::preserves(&input.target_type, &input.reason) {
        evidence::capture(&mut tx, &input.target_type, input.target_id, evidence::Cause::Reported, Some(auth.user_id)).await?
    } else {
        evidence::Preserved::default()
    };

    if alerts::is_urgent(&input.reason) {
        notices.alert(Alert::UrgentReport {
            target_type: input.target_type.clone(),
            target_id: input.target_id,
            reason: input.reason.clone(),
        });
    }

    tx.commit().await.db_err_ctx("Failed to commit transaction", "Database error")?;
    notices.send(&state).await;

    // Runs in the background: the reporter shouldn't wait on (or see
    // errors from) storage round-trips, and every failure is logged with
    // the post ID for manual follow-up. The evidence copy goes first, so it
    // reads the files before the key rotation moves them.
    {
        let state = state.clone();
        let post_id = input.target_id;
        tokio::spawn(async move {
            evidence::finish(&state, preserved, Vec::new()).await;
            if newly_hidden {
                if let Err(e) = rotate_post_media_keys(&state, post_id).await {
                    tracing::error!("Media key rotation for hidden post {} failed: {}", post_id, e.message);
                }
            }
        });
    }

    tracing::info!(
        "Report created: {} (reason={}, target={}:{})",
        report.id, input.reason, input.target_type, input.target_id
    );

    Ok((StatusCode::CREATED, Json(report)))
}

/// Moves a hidden post's media to fresh storage keys and purges the old
/// URLs from the CDN. Hiding only stops the API from handing out URLs;
/// anyone who already had one could keep loading the files straight from
/// the CDN. The files are moved rather than deleted because a hide can be
/// dismissed, and because illegal content must be preserved as evidence
/// until the report is resolved. Admin review and the owner's own view
/// keep working, since they resolve whatever keys media_assets holds.
pub async fn rotate_post_media_keys(state: &AppState, post_id: Uuid) -> Result<(), AppError> {
    let assets = sqlx::query_as::<_, (Uuid, String, String, String)>(
        "SELECT id, thumb_key, medium_key, full_key FROM media_assets WHERE post_id = $1"
    )
    .bind(post_id)
    .fetch_all(&state.db)
    .await
    .db_err("Database error")?;

    for (asset_id, thumb, medium, full) in assets {
        let new_id = Uuid::new_v4();
        let old_keys = [thumb, medium, full];
        let new_keys = old_keys.clone().map(|k| rotated_key(&k, new_id));

        // Copy first, so the files are never missing from storage.
        let mut copied = Vec::new();
        let mut copy_result = Ok(());
        for (old, new) in old_keys.iter().zip(&new_keys) {
            copy_result = copy_object(state, old, new).await;
            if copy_result.is_err() {
                break;
            }
            copied.push(new.clone());
        }

        // The full_key guard makes this a no-op if the post was deleted or
        // the keys changed underneath us; in that case (or if a copy
        // failed) the new copies are orphans and are removed again.
        let updated = match copy_result {
            Ok(()) => sqlx::query(
                r#"
                UPDATE media_assets
                SET thumb_key = $2, medium_key = $3, full_key = $4, original_key = $4
                WHERE id = $1 AND full_key = $5
                "#
            )
            .bind(asset_id)
            .bind(&new_keys[0])
            .bind(&new_keys[1])
            .bind(&new_keys[2])
            .bind(&old_keys[2])
            .execute(&state.db)
            .await
            .db_err("Database error")
            .map(|r| r.rows_affected() > 0),
            Err(e) => Err(e),
        };

        match updated {
            Ok(true) => {}
            Ok(false) => {
                for key in &copied {
                    let _ = state.storage.delete(key).await;
                }
                continue;
            }
            Err(e) => {
                for key in &copied {
                    let _ = state.storage.delete(key).await;
                }
                return Err(e);
            }
        }

        // A pending evidence copy of the old file now reads from the new one.
        for (old, new) in old_keys.iter().zip(&new_keys) {
            evidence::follow_moved_source(state, old, new).await?;
        }

        // From here on a failure leaves the old file reachable; delete_media
        // logs each one with its key for manual cleanup.
        for old in &old_keys {
            delete_media(state, old).await;
        }
    }

    tracing::info!("Rotated media keys of hidden post {}", post_id);
    Ok(())
}

/// Storage has no server-side copy across all backends, so this is a
/// download + upload. Media files are small (processed WebP variants).
pub(crate) async fn copy_object(state: &AppState, from: &str, to: &str) -> Result<(), AppError> {
    let data = state.storage.get(from).await?;
    state.storage.save(to, &data).await
}

/// "thumb/<old-id>.webp" -> "thumb/<new-id>.webp": same folder and
/// extension (save() derives the Content-Type from it), new unguessable ID.
pub(crate) fn rotated_key(old: &str, new_id: Uuid) -> String {
    let folder = old.rsplit_once('/').map(|(dir, _)| format!("{}/", dir)).unwrap_or_default();
    let ext = old.rsplit_once('.').map(|(_, ext)| format!(".{}", ext)).unwrap_or_default();
    format!("{}{}{}", folder, new_id, ext)
}

/// One pending report, as listed in the queue under its item.
#[derive(Debug, Serialize)]
pub struct QueueReport {
    pub id: Uuid,
    /// Both None once the reporter deleted their account, and for a notice
    /// from the public form (`source` "public_notice").
    pub reporter_id: Option<Uuid>,
    pub reporter_username: Option<String>,
    pub reason: String,
    pub details: Option<String>,
    pub created_at: DateTime<Utc>,
    /// "user_report", "public_notice", "own_initiative" or "authority_order".
    pub source: String,
    pub authority: Option<String>,
    pub order_reference: Option<String>,
    /// Set when the reporter asked to have a dismissal checked again.
    pub recheck_requested_at: Option<DateTime<Utc>>,
    pub recheck_note: Option<String>,
    /// For a notice from the public form: who sent it, if they said (both
    /// are optional for CSAM, Art. 16(2)(c) DSA). They hear the outcome by
    /// email.
    pub notifier_name: Option<String>,
    pub notifier_email: Option<String>,
}

/// An objection against an automatic restriction that rests on reports in
/// the group. Deciding the reports answers it (see dismiss_report).
#[derive(Debug, Serialize)]
pub struct QueueObjection {
    pub decision_id: Uuid,
    pub restriction: String,
    pub objection: Option<String>,
    pub objected_at: Option<DateTime<Utc>>,
}

/// All pending reports on one item. Dismissing or removing acts on the
/// whole group: one decision with every report, every reporter told.
#[derive(Debug, Serialize)]
pub struct ReportGroup {
    pub target_type: String,
    pub target_id: Uuid,
    /// Post caption or comment body. None for profiles, and for messages:
    /// a direct message is only shown through its evidence record, whose
    /// every opening is logged.
    pub target_preview: Option<String>,
    /// The post's first image (raw storage key). Never sent for a group
    /// with a CSAM or intimate-image report: those are opened deliberately
    /// in the evidence view, not shown in a list.
    pub target_thumb_url: Option<String>,
    /// The reported account, or the author of the reported item; None once
    /// that account is gone.
    pub target_username: Option<String>,
    /// False once the item was deleted (its evidence copy is what to review).
    pub target_exists: bool,
    /// The item's moderation_status (posts and comments).
    pub target_status: Option<String>,
    pub evidence_id: Option<Uuid>,
    pub evidence_content_deleted: Option<bool>,
    /// "critical" (CSAM, intimate images, or an authority's order), "high"
    /// (shown behind a warning) or "normal".
    pub severity: &'static str,
    /// A report in the group has waited for more than 30 days.
    pub overdue: bool,
    /// Oldest first.
    pub reports: Vec<QueueReport>,
    pub objections: Vec<QueueObjection>,
}

#[derive(sqlx::FromRow)]
struct PendingRow {
    id: Uuid,
    reporter_id: Option<Uuid>,
    reporter_username: Option<String>,
    target_type: String,
    target_id: Uuid,
    reason: String,
    details: Option<String>,
    created_at: DateTime<Utc>,
    source: String,
    authority: Option<String>,
    order_reference: Option<String>,
    recheck_requested_at: Option<DateTime<Utc>>,
    recheck_note: Option<String>,
    notifier_name: Option<String>,
    notifier_email: Option<String>,
    target_preview: Option<String>,
    target_thumb_url: Option<String>,
    target_username: Option<String>,
    target_exists: bool,
    target_status: Option<String>,
    evidence_id: Option<Uuid>,
    evidence_content_deleted: Option<bool>,
}

fn severity_rank(severity: &str) -> u8 {
    match severity {
        "critical" => 0,
        "high" => 1,
        _ => 2,
    }
}

/// GET /admin/reports (admin only) -- the review queue: pending reports
/// grouped by item, overdue groups first, then by severity (critical,
/// high, normal), then the most recent report first.
pub async fn get_reports(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Vec<ReportGroup>>, AppError> {
    require_admin(&state.db, &auth).await?;

    let rows = sqlx::query_as::<_, PendingRow>(
        r#"
        SELECT
            r.id, r.reporter_id, u_reporter.username AS reporter_username,
            r.target_type::text AS target_type, r.target_id, r.reason::text AS reason, r.details, r.created_at,
            r.source, r.authority, r.order_reference, r.recheck_requested_at, r.recheck_note,
            cn.notifier_name, cn.notifier_email,
            CASE r.target_type WHEN 'post' THEN p.caption WHEN 'comment' THEN c.body END AS target_preview,
            CASE r.target_type WHEN 'post' THEN pm.thumb_key END AS target_thumb_url,
            COALESCE(
                CASE r.target_type
                    WHEN 'post' THEN u_post.username
                    WHEN 'comment' THEN u_comment.username
                    WHEN 'user' THEN u_target.username
                    WHEN 'message' THEN u_message.username
                END,
                -- Deleted by its author: the account behind the evidence copy.
                (SELECT u.username FROM users u WHERE u.id = ev.author_id)
            ) AS target_username,
            CASE r.target_type
                WHEN 'post' THEN p.id IS NOT NULL
                WHEN 'comment' THEN c.id IS NOT NULL
                WHEN 'user' THEN u_target.id IS NOT NULL
                ELSE m.id IS NOT NULL
            END AS target_exists,
            CASE r.target_type WHEN 'post' THEN p.moderation_status::text WHEN 'comment' THEN c.moderation_status::text END
                AS target_status,
            ev.id AS evidence_id,
            ev.content_deleted_at IS NOT NULL AS evidence_content_deleted
        FROM reports r
        LEFT JOIN users u_reporter ON u_reporter.id = r.reporter_id
        LEFT JOIN content_notices cn ON cn.id = r.notice_id
        LEFT JOIN posts p ON r.target_type = 'post' AND p.id = r.target_id
        LEFT JOIN media_assets pm ON pm.post_id = p.id AND pm.sort_order = 0
        LEFT JOIN users u_post ON u_post.id = p.user_id
        LEFT JOIN comments c ON r.target_type = 'comment' AND c.id = r.target_id
        LEFT JOIN users u_comment ON u_comment.id = c.user_id
        LEFT JOIN users u_target ON r.target_type = 'user' AND u_target.id = r.target_id
        LEFT JOIN messages m ON r.target_type = 'message' AND m.id = r.target_id
        LEFT JOIN users u_message ON u_message.id = m.sender_id
        LEFT JOIN LATERAL (
            SELECT e.id, e.content_deleted_at,
                   (SELECT (v.content->'author'->>'id')::uuid FROM evidence_versions v
                    WHERE v.evidence_id = e.id ORDER BY v.captured_at DESC LIMIT 1) AS author_id
            FROM evidence_records e
            WHERE e.target_type = r.target_type AND e.target_id = r.target_id AND e.purged_at IS NULL
            ORDER BY e.created_at DESC LIMIT 1
        ) ev ON true
        WHERE r.status = 'pending'
        ORDER BY r.created_at, r.id
        "#
    )
    .fetch_all(&state.db)
    .await
    .db_err("Database error")?;

    let report_ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();
    let objections = sqlx::query_as::<_, (String, Uuid, Uuid, String, Option<String>, Option<DateTime<Utc>>)>(
        r#"
        SELECT d.target_type::text, d.target_id, d.id, d.restriction, d.objection, d.objected_at
        FROM moderation_decisions d
        WHERE d.objection_status = 'pending' AND d.automated AND d.lifted_at IS NULL AND d.superseded_by IS NULL
          AND d.report_ids && $1::uuid[]
        ORDER BY d.objected_at
        "#,
    )
    .bind(&report_ids)
    .fetch_all(&state.db)
    .await
    .db_err("Database error")?;

    let now = Utc::now();
    let mut groups: Vec<ReportGroup> = Vec::new();
    for row in rows {
        let group = match groups.iter_mut().find(|g| g.target_type == row.target_type && g.target_id == row.target_id) {
            Some(group) => group,
            None => {
                groups.push(ReportGroup {
                    target_type: row.target_type.clone(),
                    target_id: row.target_id,
                    target_preview: row.target_preview.clone(),
                    target_thumb_url: row.target_thumb_url.clone(),
                    target_username: row.target_username.clone(),
                    target_exists: row.target_exists,
                    target_status: row.target_status.clone(),
                    evidence_id: row.evidence_id,
                    evidence_content_deleted: row.evidence_content_deleted,
                    severity: "normal",
                    overdue: false,
                    reports: Vec::new(),
                    objections: Vec::new(),
                });
                groups.last_mut().expect("just pushed")
            }
        };
        let severity = if is_critical(&row.reason) || row.source == "authority_order" {
            "critical"
        } else if is_high_severity(&row.reason) {
            "high"
        } else {
            "normal"
        };
        if severity_rank(severity) < severity_rank(group.severity) {
            group.severity = severity;
        }
        if is_critical(&row.reason) {
            group.target_thumb_url = None;
        }
        group.overdue |= row.created_at < now - chrono::Duration::days(30);
        group.reports.push(QueueReport {
            id: row.id,
            reporter_id: row.reporter_id,
            reporter_username: row.reporter_username,
            reason: row.reason,
            details: row.details,
            created_at: row.created_at,
            source: row.source,
            authority: row.authority,
            order_reference: row.order_reference,
            recheck_requested_at: row.recheck_requested_at,
            recheck_note: row.recheck_note,
            notifier_name: row.notifier_name,
            notifier_email: row.notifier_email,
        });
    }
    for (target_type, target_id, decision_id, restriction, objection, objected_at) in objections {
        if let Some(group) = groups.iter_mut().find(|g| g.target_type == target_type && g.target_id == target_id) {
            group.objections.push(QueueObjection { decision_id, restriction, objection, objected_at });
        }
    }

    groups.sort_by_key(|g| {
        let latest = g.reports.iter().map(|r| r.created_at).max();
        (!g.overdue, severity_rank(g.severity), std::cmp::Reverse(latest))
    });
    Ok(Json(groups.resolve_media(&state.storage)))
}

/// The body of a dismissal or removal. The frontend always sends at least
/// `{}`, so this is a required Json body.
#[derive(Debug, Deserialize, Default)]
pub struct ReviewNote {
    /// Internal, never shown to the reporter or the author.
    pub note: Option<String>,
    /// Only for a removal: the violation type from the catalog (or "none"),
    /// which sets the author's strike (standing.rs). Defaults to the
    /// reported reason's first type.
    pub violation: Option<String>,
    /// Required when `violation` belongs to another reason than any
    /// report's, or is "none". Internal.
    pub justification: Option<String>,
    /// Only for a dismissal: dismiss just this report, not the other
    /// pending ones on the same item (e.g. when their reasons differ).
    #[serde(default)]
    pub only_this: bool,
    /// Only for a dismissal: the answer to a pending objection against an
    /// automatic restriction the dismissal lifts. Shown to the author.
    pub objection_response: Option<String>,
}

fn checked_note(note: &Option<String>, what: &str) -> Result<(), AppError> {
    if note.as_ref().is_some_and(|n| n.chars().count() > 1000) {
        return Err(AppError::bad_request(format!("{} must be under 1000 characters", what)));
    }
    Ok(())
}

/// The pending reports a decision acts on.
struct Group {
    target_type: String,
    target_id: Uuid,
    ids: Vec<Uuid>,
    /// Distinct reasons, the given report's first.
    reasons: Vec<String>,
}

/// Locks a pending report and, unless `only_this`, every other pending
/// report on the same item. FOR UPDATE makes two admins deciding at once
/// serialize, so nothing is decided twice.
async fn lock_group(tx: &mut PgConnection, report_id: Uuid, only_this: bool) -> Result<Group, AppError> {
    let (target_type, target_id, reason) = sqlx::query_as::<_, (String, Uuid, String)>(
        "SELECT target_type::text, target_id, reason::text FROM reports WHERE id = $1 AND status = 'pending' FOR UPDATE",
    )
    .bind(report_id)
    .fetch_optional(&mut *tx)
    .await
    .db_err("Database error")?
    .ok_or_else(|| AppError::not_found("Report not found or already reviewed"))?;

    let mut ids = vec![report_id];
    let mut reasons = vec![reason];
    if !only_this {
        let others = sqlx::query_as::<_, (Uuid, String)>(
            r#"
            SELECT id, reason::text FROM reports
            WHERE target_type = $1::report_target_type AND target_id = $2 AND status = 'pending' AND id != $3
            ORDER BY created_at
            FOR UPDATE
            "#,
        )
        .bind(&target_type)
        .bind(target_id)
        .bind(report_id)
        .fetch_all(&mut *tx)
        .await
        .db_err("Database error")?;
        for (id, reason) in others {
            ids.push(id);
            if !reasons.contains(&reason) {
                reasons.push(reason);
            }
        }
    }
    Ok(Group { target_type, target_id, ids, reasons })
}

/// POST /admin/reports/:id/dismiss (admin only) -- closes the report and
/// every other pending report on the same item (or just this one, with
/// `only_this`), and lifts the automatic restrictions that rested on them
/// alone. Restrictions from other reports still pending, from a rights
/// claim or from the team stay in force: the item's status is derived from
/// what is left (moderation::refresh_status).
pub async fn dismiss_report(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(report_id): Path<Uuid>,
    Json(input): Json<ReviewNote>,
) -> Result<StatusCode, AppError> {
    require_admin(&state.db, &auth).await?;
    checked_note(&input.note, "Review note")?;
    checked_note(&input.objection_response, "The answer to the objection")?;

    // One transaction, so the evidence decision below always matches the
    // report states it was derived from.
    let mut tx = state.db.begin().await.db_err_ctx("Failed to start transaction", "Database error")?;
    let group = lock_group(&mut tx, report_id, input.only_this).await?;

    let mut notices =
        moderation::close_reports(&mut tx, &group.ids, "dismissed", "no_violation", Some(auth.user_id), input.note.as_deref())
            .await?;
    notices.extend(
        moderation::lift_for_dismissed(
            &mut tx,
            &group.target_type,
            group.target_id,
            &group.ids,
            auth.user_id,
            input.objection_response.as_deref(),
        )
        .await?,
    );
    moderation::refresh_status(&mut tx, &group.target_type, group.target_id).await?;

    // Content its author deleted while reported was preserved; if this was
    // the last report keeping it, that evidence is now decided.
    evidence::decide(&mut tx, &group.target_type, group.target_id, auth.user_id, input.note.as_deref()).await?;

    tx.commit().await.db_err_ctx("Failed to commit transaction", "Database error")?;
    notices.send(&state).await;

    tracing::info!("{} report(s) on {} {} dismissed by admin {}", group.ids.len(), group.target_type, group.target_id, auth.user_id);
    Ok(StatusCode::NO_CONTENT)
}

/// POST /admin/reports/:id/remove (admin only) -- the team confirms a
/// violation: one decision for every pending report on the item, with the
/// admin's classification (the author's strike), and every reporter told.
///
/// A post or comment is removed, not deleted: it disappears for everyone
/// but stays until the objection window has passed, so an accepted
/// objection can restore it (retention.rs deletes it then; CSAM as soon as
/// its evidence copy is made). A message is deleted for both sides at once.
/// Content its author already deleted is decided on its evidence copy, so
/// deleting doesn't escape the strike.
///
/// For target_type "user" this only works once the account is gone: it then
/// confirms the violation, which keeps the preserved profile as evidence. A
/// live account gets a measure (standing page) or a profile removal.
pub async fn remove_reported_content(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(report_id): Path<Uuid>,
    Json(input): Json<ReviewNote>,
) -> Result<StatusCode, AppError> {
    require_admin(&state.db, &auth).await?;
    checked_note(&input.note, "Review note")?;

    let mut tx = state.db.begin().await.db_err_ctx("Failed to start transaction", "Database error")?;
    let group = lock_group(&mut tx, report_id, false).await?;

    if group.target_type == "user" {
        let exists = sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM users WHERE id = $1)")
            .bind(group.target_id)
            .fetch_one(&mut *tx)
            .await
            .db_err("Database error")?;
        if exists {
            return Err(AppError::bad_request(
                "Account-level action isn't available from the report queue -- decide a measure or remove parts of the profile",
            ));
        }
    }

    let reasons: Vec<&str> = group.reasons.iter().map(String::as_str).collect();
    let classification = standing::classify(&reasons, input.violation.as_deref(), input.justification.as_deref())?;
    // The statement cites the ground of what the team found, which can
    // differ from what the reporters picked.
    let classified = classification.violation;
    let decided_reason = classified.map(|v| v.reason.to_string()).unwrap_or_else(|| group.reasons[0].clone());
    let source = moderation::source_of_reports(&mut tx, &group.ids).await?;

    let recorded = moderation::record_decision(&mut tx, NewDecision {
        decided_by: Some(auth.user_id),
        report_ids: group.ids.clone(),
        source,
        classification: Some(classification),
        ..NewDecision::base(&group.target_type, group.target_id, Restriction::Removed, &decided_reason)
    })
    .await?;
    let mut notices = recorded.notices;

    if recorded.decision_id.is_none() {
        // Gone, and nothing of it was kept (no likely-illegal report): there
        // is nothing left to decide on.
        notices.extend(moderation::close_reports(&mut tx, &group.ids, "obsolete", "obsolete", Some(auth.user_id), input.note.as_deref()).await?);
        tx.commit().await.db_err_ctx("Failed to commit transaction", "Database error")?;
        notices.send(&state).await;
        return Ok(StatusCode::NO_CONTENT);
    }

    // The reporters' reasons decided what was preserved when they reported;
    // the team's classification can find more (a "spam" report that is
    // Holocaust denial), and what it finds likely illegal is preserved too,
    // flagged for the authorities where the catalog says so.
    let mut preserved = evidence::Preserved::default();
    if let Some(v) = classified {
        preserved.extend(
            evidence::preserve_classified(&mut tx, &group.target_type, group.target_id, v.reason, v.authority_report.as_db(), auth.user_id)
                .await?,
        );
        if v.authority_report == AuthorityReport::Required {
            let evidence_id = sqlx::query_scalar::<_, Uuid>(
                r#"
                SELECT id FROM evidence_records WHERE target_type = $1::report_target_type AND target_id = $2 AND purged_at IS NULL
                ORDER BY created_at DESC LIMIT 1
                "#,
            )
            .bind(&group.target_type)
            .bind(group.target_id)
            .fetch_optional(&mut *tx)
            .await
            .db_err("Database error")?;
            notices.alert(Alert::AuthorityReportRequired {
                target_type: group.target_type.clone(),
                target_id: group.target_id,
                evidence_id,
            });
        }
    }

    let mut rotate = false;
    if !recorded.content_gone {
        match group.target_type.as_str() {
            "post" | "comment" => {
                if let Some((old, _)) = moderation::refresh_status(&mut tx, &group.target_type, group.target_id).await? {
                    // A hidden post's files were already moved when it was hidden.
                    rotate = group.target_type == "post" && old != "hidden";
                }
            }
            "message" => {
                evidence::mark_deleted(&mut tx, "message", group.target_id, evidence::Trigger::ModerationRemoval).await?;
                sqlx::query("DELETE FROM messages WHERE id = $1")
                    .bind(group.target_id)
                    .execute(&mut *tx)
                    .await
                    .db_err("Failed to remove content")?;
            }
            _ => {}
        }
    }

    notices.extend(
        moderation::close_reports(&mut tx, &group.ids, "actioned", "removed", Some(auth.user_id), input.note.as_deref()).await?,
    );
    evidence::decide(&mut tx, &group.target_type, group.target_id, auth.user_id, input.note.as_deref()).await?;

    tx.commit().await.db_err_ctx("Failed to commit transaction", "Database error")?;
    notices.send(&state).await;

    // The evidence copy first, so it reads the files before the rotation
    // moves them; removed content's old links stop working once moved.
    evidence::finish(&state, preserved, Vec::new()).await;
    if rotate {
        let state = state.clone();
        let post_id = group.target_id;
        tokio::spawn(async move {
            if let Err(e) = rotate_post_media_keys(&state, post_id).await {
                tracing::error!("Media key rotation for removed post {} failed: {}", post_id, e.message);
            }
        });
    }

    tracing::info!(
        "{} report(s) on {} {} actioned (removed) by admin {}",
        group.ids.len(), group.target_type, group.target_id, auth.user_id
    );
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
pub struct CreateCaseRequest {
    /// "post", "comment" or "user".
    pub target_type: String,
    pub target_id: Uuid,
    pub reason: String,
    /// What the team found, or what the order says. Internal.
    pub details: Option<String>,
    /// "own_initiative" or "authority_order".
    pub source: String,
    /// For an order: the authority that issued it (required) and its
    /// reference.
    pub authority: Option<String>,
    pub order_reference: Option<String>,
}

/// POST /admin/cases (admin only) -- opens a case without a report: content
/// the team came across itself, or an authority's order to act against it
/// (Art. 9 DSA; a removal order for terrorist content has to be carried out
/// within one hour, Regulation 2021/784 -- see docs/admin-runbook.md). The
/// case is a report from the admin with that source, decided in the queue
/// like any other, and the statement names where it came from (Art.
/// 17(3)(b)). Returns the report's id.
pub async fn create_case(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<CreateCaseRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    require_admin(&state.db, &auth).await?;
    if !matches!(input.target_type.as_str(), "post" | "comment" | "user") {
        return Err(AppError::bad_request("Invalid target_type"));
    }
    if !VALID_REASONS.contains(&input.reason.as_str()) {
        return Err(AppError::bad_request("Invalid reason"));
    }
    let text = |value: &Option<String>, what: &str, max: usize| -> Result<Option<String>, AppError> {
        let value = value.as_deref().map(str::trim).filter(|v| !v.is_empty()).map(str::to_string);
        if value.as_ref().is_some_and(|v| v.chars().count() > max) {
            return Err(AppError::bad_request(format!("{} must be under {} characters", what, max)));
        }
        Ok(value)
    };
    let details = text(&input.details, "Details", 2000)?;
    let (authority, reference) = match input.source.as_str() {
        "own_initiative" => (None, None),
        "authority_order" => (
            Some(text(&input.authority, "The authority", 200)?.ok_or_else(|| AppError::bad_request("Name the authority"))?),
            text(&input.order_reference, "The reference", 200)?,
        ),
        _ => return Err(AppError::bad_request("Invalid source")),
    };

    let mut tx = state.db.begin().await.db_err_ctx("Failed to start transaction", "Database error")?;
    let exists = sqlx::query_scalar::<_, bool>(match input.target_type.as_str() {
        "post" => "SELECT EXISTS(SELECT 1 FROM posts WHERE id = $1 AND moderation_status != 'removed')",
        "comment" => "SELECT EXISTS(SELECT 1 FROM comments WHERE id = $1 AND moderation_status != 'removed')",
        _ => "SELECT EXISTS(SELECT 1 FROM users WHERE id = $1)",
    })
    .bind(input.target_id)
    .fetch_one(&mut *tx)
    .await
    .db_err("Database error")?;
    if !exists {
        return Err(AppError::not_found("Not found"));
    }

    let report_id = sqlx::query_scalar::<_, Uuid>(
        r#"
        INSERT INTO reports (reporter_id, target_type, target_id, reason, details, source, authority, order_reference)
        VALUES ($1, $2::report_target_type, $3, $4::report_reason, $5, $6, $7, $8)
        RETURNING id
        "#,
    )
    .bind(auth.user_id)
    .bind(&input.target_type)
    .bind(input.target_id)
    .bind(&input.reason)
    .bind(details)
    .bind(&input.source)
    .bind(authority)
    .bind(reference)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| {
        if e.as_database_error().and_then(|d| d.constraint()) == Some("idx_reports_pending_once") {
            AppError::conflict("You already have an open case or report on this.")
        } else {
            tracing::error!("Failed to open case: {}", e);
            AppError::internal("Failed to open case")
        }
    })?;

    let preserved = if evidence::preserves(&input.target_type, &input.reason) {
        evidence::capture(&mut tx, &input.target_type, input.target_id, evidence::Cause::Reported, Some(auth.user_id)).await?
    } else {
        evidence::Preserved::default()
    };
    tx.commit().await.db_err_ctx("Failed to commit transaction", "Database error")?;
    evidence::finish(&state, preserved, Vec::new()).await;

    tracing::info!("Case {} opened by admin {} ({}, {} {})", report_id, auth.user_id, input.source, input.target_type, input.target_id);
    Ok((StatusCode::CREATED, Json(serde_json::json!({ "report_id": report_id }))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotated_key_keeps_folder_and_extension() {
        let id = Uuid::new_v4();
        assert_eq!(rotated_key("thumb/0190-old.webp", id), format!("thumb/{}.webp", id));
        assert_eq!(rotated_key("full/abc.jpg", id), format!("full/{}.jpg", id));
        assert_eq!(rotated_key("plain", id), id.to_string());
    }
}
