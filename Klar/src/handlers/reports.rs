/// Content reporting & moderation.
///
/// POST /reports is available to any authenticated user. Everything else
/// here (the review queue, dismiss, remove) is gated to a single admin
/// user via the ADMIN_USER_ID env var -- a plain UUID comparison rather
/// than a full roles system, proportionate to a solo-dev, pre-launch
/// scale. Worth revisiting once there's more than one person moderating.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::errors::AppError;
use crate::handlers::auth::AppState;
use crate::models::{AdminReportRow, CreateReportRequest, ReportRow};

const VALID_REASONS: &[&str] = &[
    "spam", "harassment", "hate_speech", "violence",
    "self_harm", "sexual_content", "csam", "impersonation", "other",
];
const VALID_TARGET_TYPES: &[&str] = &["post", "comment", "user"];

/// CSAM gets zero tolerance: a single report hides the content
/// immediately, with no "view anyway" interstitial and no waiting for a
/// second corroborating report.
fn is_critical(reason: &str) -> bool {
    reason == "csam"
}

/// Serious enough to warn readers, not serious enough to let one report
/// unilaterally remove someone's content outright -- report pile-ons are
/// a real abuse vector, so these get an interstitial ("may violate our
/// guidelines, pending review") rather than outright hiding.
fn is_high_severity(reason: &str) -> bool {
    matches!(reason, "violence" | "self_harm" | "sexual_content")
}

fn require_admin(auth: &AuthUser) -> Result<(), AppError> {
    let admin_id = std::env::var("ADMIN_USER_ID")
        .ok()
        .and_then(|s| Uuid::parse_str(&s).ok());

    match admin_id {
        Some(id) if id == auth.user_id => Ok(()),
        _ => Err(AppError::forbidden("Admin access required")),
    }
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

    // Verify the target actually exists, so someone can't report an
    // arbitrary/garbage UUID, and grab enough info to reject
    // self-reports on the "user" path.
    match input.target_type.as_str() {
        "post" => {
            let exists = sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM posts WHERE id = $1)"
            )
            .bind(input.target_id)
            .fetch_one(&state.db)
            .await
            .map_err(|e| { tracing::error!("Database error: {}", e); AppError::internal("Database error") })?;
            if !exists {
                return Err(AppError::not_found("Post not found"));
            }
        }
        "comment" => {
            let exists = sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM comments WHERE id = $1)"
            )
            .bind(input.target_id)
            .fetch_one(&state.db)
            .await
            .map_err(|e| { tracing::error!("Database error: {}", e); AppError::internal("Database error") })?;
            if !exists {
                return Err(AppError::not_found("Comment not found"));
            }
        }
        "user" => {
            if input.target_id == auth.user_id {
                return Err(AppError::bad_request("You can't report yourself"));
            }
            let exists = sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM users WHERE id = $1)"
            )
            .bind(input.target_id)
            .fetch_one(&state.db)
            .await
            .map_err(|e| { tracing::error!("Database error: {}", e); AppError::internal("Database error") })?;
            if !exists {
                return Err(AppError::not_found("User not found"));
            }
        }
        _ => unreachable!("validated above"),
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
    .bind(&input.details)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Failed to create report: {}", e);
        AppError::internal("Failed to submit report")
    })?;

    // Auto-moderation: only posts/comments have a moderation_status to
    // update (a "user" report has no content to hide -- it just queues
    // for admin review at whatever priority its reason implies).
    if input.target_type == "post" {
        if is_critical(&input.reason) {
            sqlx::query("UPDATE posts SET moderation_status = 'hidden' WHERE id = $1")
                .bind(input.target_id).execute(&state.db).await
                .map_err(|e| { tracing::error!("Failed to auto-hide post: {}", e); AppError::internal("Database error") })?;
        } else if is_high_severity(&input.reason) {
            // Never downgrade an already-hidden (CSAM) post back to
            // merely "flagged".
            sqlx::query("UPDATE posts SET moderation_status = 'flagged' WHERE id = $1 AND moderation_status = 'visible'")
                .bind(input.target_id).execute(&state.db).await
                .map_err(|e| { tracing::error!("Failed to flag post: {}", e); AppError::internal("Database error") })?;
        }
    } else if input.target_type == "comment" {
        if is_critical(&input.reason) {
            sqlx::query("UPDATE comments SET moderation_status = 'hidden' WHERE id = $1")
                .bind(input.target_id).execute(&state.db).await
                .map_err(|e| { tracing::error!("Failed to auto-hide comment: {}", e); AppError::internal("Database error") })?;
        } else if is_high_severity(&input.reason) {
            sqlx::query("UPDATE comments SET moderation_status = 'flagged' WHERE id = $1 AND moderation_status = 'visible'")
                .bind(input.target_id).execute(&state.db).await
                .map_err(|e| { tracing::error!("Failed to flag comment: {}", e); AppError::internal("Database error") })?;
        }
    }

    tracing::info!(
        "Report created: {} (reason={}, target={}:{})",
        report.id, input.reason, input.target_type, input.target_id
    );

    Ok((StatusCode::CREATED, Json(report)))
}

/// GET /admin/reports (admin only) -- the review queue, critical
/// (CSAM) reports first, then high-severity, then everything else, most
/// recent within each tier. Severity isn't a natural SQL sort (it
/// depends on `reason`), so it's computed with a CASE expression rather
/// than requiring a denormalized severity column that could drift out
/// of sync with the reason lists above.
pub async fn get_reports(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Vec<AdminReportRow>>, AppError> {
    require_admin(&auth)?;

    let reports = sqlx::query_as::<_, AdminReportRow>(
        r#"
        SELECT
            r.id, r.reporter_id, u_reporter.username as reporter_username,
            r.target_type::text, r.target_id, r.reason::text, r.details,
            r.status::text, r.created_at,
            CASE r.target_type
                WHEN 'post' THEN p.caption
                WHEN 'comment' THEN c.body
                ELSE NULL
            END as target_preview,
            CASE r.target_type
                WHEN 'post' THEN pm.thumb_key
                ELSE NULL
            END as target_thumb_url,
            CASE r.target_type
                WHEN 'post' THEN u_post.username
                WHEN 'comment' THEN u_comment.username
                WHEN 'user' THEN u_target.username
            END as target_username
        FROM reports r
        JOIN users u_reporter ON u_reporter.id = r.reporter_id
        LEFT JOIN posts p ON r.target_type = 'post' AND p.id = r.target_id
        LEFT JOIN media_assets pm ON pm.post_id = p.id AND pm.sort_order = 0
        LEFT JOIN users u_post ON u_post.id = p.user_id
        LEFT JOIN comments c ON r.target_type = 'comment' AND c.id = r.target_id
        LEFT JOIN users u_comment ON u_comment.id = c.user_id
        LEFT JOIN users u_target ON r.target_type = 'user' AND u_target.id = r.target_id
        WHERE r.status = 'pending'
        ORDER BY
            CASE
                WHEN r.reason = 'csam' THEN 0
                WHEN r.reason IN ('violence', 'self_harm', 'sexual_content') THEN 1
                ELSE 2
            END,
            r.created_at DESC
        "#
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Database error: {}", e);
        AppError::internal("Database error")
    })?;

    Ok(Json(reports))
}

/// Optional note an admin can attach when dismissing or removing a
/// report -- e.g. "false report, content is fine" or "removed per policy
/// X". Frontend always sends at least `{}` (same pattern as
/// auth.rs's LogoutRequest), so this is a required Json body, not an
/// Option<Json<..>>.
#[derive(Debug, Deserialize, Default)]
pub struct ReviewNote {
    pub note: Option<String>,
}

/// POST /admin/reports/:id/dismiss (admin only) -- clears the report and,
/// if it's a post/comment, reverts moderation_status back to visible.
/// Note: this reverts regardless of whether *other* pending reports
/// exist on the same content -- simple and correct for the common case
/// (one report, one decision); if multiple reports on the same item ever
/// need independent tracking, that's a deliberate scope call for later.
pub async fn dismiss_report(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(report_id): Path<Uuid>,
    Json(input): Json<ReviewNote>,
) -> Result<StatusCode, AppError> {
    require_admin(&auth)?;

    if let Some(note) = &input.note {
        if note.chars().count() > 1000 {
            return Err(AppError::bad_request("Review note must be under 1000 characters"));
        }
    }

    let report = sqlx::query_as::<_, (String, Uuid)>(
        "SELECT target_type::text, target_id FROM reports WHERE id = $1 AND status = 'pending'"
    )
    .bind(report_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| { tracing::error!("Database error: {}", e); AppError::internal("Database error") })?
    .ok_or_else(|| AppError::not_found("Report not found or already reviewed"))?;

    let (target_type, target_id) = report;

    sqlx::query(
        "UPDATE reports SET status = 'dismissed', reviewed_at = NOW(), reviewed_by = $1, review_note = $2 WHERE id = $3"
    )
    .bind(auth.user_id)
    .bind(&input.note)
    .bind(report_id)
    .execute(&state.db)
    .await
    .map_err(|e| { tracing::error!("Failed to dismiss report: {}", e); AppError::internal("Database error") })?;

    if target_type == "post" {
        sqlx::query("UPDATE posts SET moderation_status = 'visible' WHERE id = $1")
            .bind(target_id).execute(&state.db).await
            .map_err(|e| { tracing::error!("Database error: {}", e); AppError::internal("Database error") })?;
    } else if target_type == "comment" {
        sqlx::query("UPDATE comments SET moderation_status = 'visible' WHERE id = $1")
            .bind(target_id).execute(&state.db).await
            .map_err(|e| { tracing::error!("Database error: {}", e); AppError::internal("Database error") })?;
    }

    Ok(StatusCode::NO_CONTENT)
}

/// POST /admin/reports/:id/remove (admin only) -- deletes the reported
/// post/comment outright and marks the report actioned. Not available
/// for target_type = "user" -- account-level action (suspension,
/// deletion) is a bigger, separate decision than a single-click queue
/// action, so it isn't wired up here on purpose.
pub async fn remove_reported_content(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(report_id): Path<Uuid>,
    Json(input): Json<ReviewNote>,
) -> Result<StatusCode, AppError> {
    require_admin(&auth)?;

    if let Some(note) = &input.note {
        if note.chars().count() > 1000 {
            return Err(AppError::bad_request("Review note must be under 1000 characters"));
        }
    }

    let report = sqlx::query_as::<_, (String, Uuid)>(
        "SELECT target_type::text, target_id FROM reports WHERE id = $1 AND status = 'pending'"
    )
    .bind(report_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| { tracing::error!("Database error: {}", e); AppError::internal("Database error") })?
    .ok_or_else(|| AppError::not_found("Report not found or already reviewed"))?;

    let (target_type, target_id) = report;

    match target_type.as_str() {
        "post" => {
            // Mirror posts::delete_post's cleanup: fetch media keys
            // before the row (and its CASCADE) removes them, delete the
            // post, then best-effort delete the actual files.
            let media_keys = sqlx::query_as::<_, (Option<String>, Option<String>, Option<String>)>(
                "SELECT thumb_key, medium_key, full_key FROM media_assets WHERE post_id = $1"
            )
            .bind(target_id)
            .fetch_all(&state.db)
            .await
            .map_err(|e| { tracing::error!("Database error: {}", e); AppError::internal("Database error") })?;

            let owner_id = sqlx::query_scalar::<_, Uuid>("SELECT user_id FROM posts WHERE id = $1")
                .bind(target_id)
                .fetch_optional(&state.db)
                .await
                .map_err(|e| { tracing::error!("Database error: {}", e); AppError::internal("Database error") })?;

            sqlx::query("DELETE FROM posts WHERE id = $1")
                .bind(target_id).execute(&state.db).await
                .map_err(|e| { tracing::error!("Failed to delete post: {}", e); AppError::internal("Failed to remove content") })?;

            if let Some(owner_id) = owner_id {
                sqlx::query("UPDATE users SET post_count = GREATEST(post_count - 1, 0) WHERE id = $1")
                    .bind(owner_id).execute(&state.db).await
                    .map_err(|e| { tracing::error!("Failed to update post_count: {}", e); AppError::internal("Database error") })?;
            }

            for (thumb, medium, full) in media_keys {
                if let Some(t) = thumb { let _ = state.storage.delete(&t).await; }
                if let Some(m) = medium { let _ = state.storage.delete(&m).await; }
                if let Some(f) = full { let _ = state.storage.delete(&f).await; }
            }
        }
        "comment" => {
            sqlx::query("DELETE FROM comments WHERE id = $1")
                .bind(target_id).execute(&state.db).await
                .map_err(|e| { tracing::error!("Failed to delete comment: {}", e); AppError::internal("Failed to remove content") })?;
        }
        "user" => {
            return Err(AppError::bad_request(
                "Account-level action isn't available from the report queue -- review the account directly"
            ));
        }
        _ => unreachable!("validated at creation"),
    }

    sqlx::query(
        "UPDATE reports SET status = 'actioned', reviewed_at = NOW(), reviewed_by = $1, review_note = $2 WHERE id = $3"
    )
    .bind(auth.user_id)
    .bind(&input.note)
    .bind(report_id)
    .execute(&state.db)
    .await
    .map_err(|e| { tracing::error!("Failed to update report: {}", e); AppError::internal("Database error") })?;

    tracing::info!("Report {} actioned (content removed) by admin {}", report_id, auth.user_id);
    Ok(StatusCode::NO_CONTENT)
}
