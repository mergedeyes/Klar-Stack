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
use serde::Deserialize;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::errors::AppError;
use crate::handlers::auth::AppState;
use crate::handlers::posts::delete_post_with_media;
use crate::models::{AdminReportRow, CreateReportRequest, ReportRow};
use crate::utils::{delete_media, is_admin_email, DbResultExt, ResolveMedia};

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

async fn require_admin(db: &sqlx::PgPool, auth: &AuthUser) -> Result<(), AppError> {
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
            .db_err("Database error")?;
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
            .db_err("Database error")?;
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
            .db_err("Database error")?;
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
    .db_err("Failed to submit report")?;

    // Auto-moderation: only posts/comments have a moderation_status to
    // update (a "user" report has no content to hide -- it just queues
    // for admin review at whatever priority its reason implies).
    if input.target_type == "post" {
        if is_critical(&input.reason) {
            // Only the transition to hidden rotates the media keys, so a
            // second CSAM report on an already-hidden post doesn't move
            // the files again.
            let newly_hidden = sqlx::query(
                "UPDATE posts SET moderation_status = 'hidden' WHERE id = $1 AND moderation_status != 'hidden'"
            )
                .bind(input.target_id).execute(&state.db).await
                .db_err_ctx("Failed to auto-hide post", "Database error")?
                .rows_affected() > 0;

            // Runs in the background: the reporter shouldn't wait on (or
            // see errors from) storage round-trips, and every failure is
            // logged with the post ID for manual follow-up.
            if newly_hidden {
                let state = state.clone();
                let post_id = input.target_id;
                tokio::spawn(async move {
                    if let Err(e) = rotate_post_media_keys(&state, post_id).await {
                        tracing::error!("Media key rotation for hidden post {} failed: {}", post_id, e.message);
                    }
                });
            }
        } else if is_high_severity(&input.reason) {
            // Never downgrade an already-hidden (CSAM) post back to
            // merely "flagged".
            sqlx::query("UPDATE posts SET moderation_status = 'flagged' WHERE id = $1 AND moderation_status = 'visible'")
                .bind(input.target_id).execute(&state.db).await
                .db_err_ctx("Failed to flag post", "Database error")?;
        }
    } else if input.target_type == "comment" {
        if is_critical(&input.reason) {
            sqlx::query("UPDATE comments SET moderation_status = 'hidden' WHERE id = $1")
                .bind(input.target_id).execute(&state.db).await
                .db_err_ctx("Failed to auto-hide comment", "Database error")?;
        } else if is_high_severity(&input.reason) {
            sqlx::query("UPDATE comments SET moderation_status = 'flagged' WHERE id = $1 AND moderation_status = 'visible'")
                .bind(input.target_id).execute(&state.db).await
                .db_err_ctx("Failed to flag comment", "Database error")?;
        }
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
async fn rotate_post_media_keys(state: &AppState, post_id: Uuid) -> Result<(), AppError> {
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
async fn copy_object(state: &AppState, from: &str, to: &str) -> Result<(), AppError> {
    let data = state.storage.get(from).await?;
    state.storage.save(to, &data).await
}

/// "thumb/<old-id>.webp" -> "thumb/<new-id>.webp": same folder and
/// extension (save() derives the Content-Type from it), new unguessable ID.
fn rotated_key(old: &str, new_id: Uuid) -> String {
    let folder = old.rsplit_once('/').map(|(dir, _)| format!("{}/", dir)).unwrap_or_default();
    let ext = old.rsplit_once('.').map(|(_, ext)| format!(".{}", ext)).unwrap_or_default();
    format!("{}{}{}", folder, new_id, ext)
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
    require_admin(&state.db, &auth).await?;

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
        LEFT JOIN users u_reporter ON u_reporter.id = r.reporter_id
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
    .db_err("Database error")?;

    Ok(Json(reports.resolve_media(&state.storage)))
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
    require_admin(&state.db, &auth).await?;

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
    .db_err("Database error")?
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
    .db_err_ctx("Failed to dismiss report", "Database error")?;

    if target_type == "post" {
        sqlx::query("UPDATE posts SET moderation_status = 'visible' WHERE id = $1")
            .bind(target_id).execute(&state.db).await
            .db_err("Database error")?;
    } else if target_type == "comment" {
        sqlx::query("UPDATE comments SET moderation_status = 'visible' WHERE id = $1")
            .bind(target_id).execute(&state.db).await
            .db_err("Database error")?;
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
    require_admin(&state.db, &auth).await?;

    if let Some(note) = &input.note {
        if note.chars().count() > 1000 {
            return Err(AppError::bad_request("Review note must be under 1000 characters"));
        }
    }

    // Everything below runs in one transaction: the content deletion, the
    // denormalized counters and the report status change either all
    // happen or none do. FOR UPDATE makes two admins clicking "remove" on
    // the same report at once serialize, so the counters are only
    // decremented once.
    let mut tx = state.db.begin().await.db_err_ctx("Failed to start transaction", "Database error")?;

    let report = sqlx::query_as::<_, (String, Uuid)>(
        "SELECT target_type::text, target_id FROM reports WHERE id = $1 AND status = 'pending' FOR UPDATE"
    )
    .bind(report_id)
    .fetch_optional(&mut *tx)
    .await
    .db_err("Database error")?
    .ok_or_else(|| AppError::not_found("Report not found or already reviewed"))?;

    let (target_type, target_id) = report;

    // Storage objects can't take part in the transaction, so their keys
    // are collected here and the files deleted only after commit.
    let mut media_keys = Vec::new();

    match target_type.as_str() {
        "post" => {
            // None: the post is already gone (deleted by its author),
            // so there's nothing to remove.
            media_keys = delete_post_with_media(&mut tx, target_id).await?.unwrap_or_default();
        }
        "comment" => {
            // Replies cascade-delete with their parent, so comment_count
            // drops by the whole subtree -- same as comments::delete_comment.
            let removed = sqlx::query_as::<_, (Uuid, i64)>(
                r#"
                WITH RECURSIVE subtree AS (
                    SELECT id FROM comments WHERE id = $1
                    UNION ALL
                    SELECT c.id FROM comments c JOIN subtree s ON c.parent_comment_id = s.id
                )
                SELECT c.post_id, (SELECT COUNT(*) FROM subtree)
                FROM comments c WHERE c.id = $1
                "#
            )
            .bind(target_id)
            .fetch_optional(&mut *tx)
            .await
            .db_err_ctx("Failed to count comment subtree", "Database error")?;

            // None: the comment is already gone (deleted by its author or
            // with its post), so there's nothing to remove or recount.
            if let Some((post_id, removed_count)) = removed {
                sqlx::query("DELETE FROM comments WHERE id = $1")
                    .bind(target_id).execute(&mut *tx).await
                    .db_err("Failed to remove content")?;

                sqlx::query("UPDATE posts SET comment_count = GREATEST(comment_count - $1, 0) WHERE id = $2")
                    .bind(removed_count)
                    .bind(post_id)
                    .execute(&mut *tx)
                    .await
                    .db_err_ctx("Failed to update comment_count", "Database error")?;
            }
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
    .execute(&mut *tx)
    .await
    .db_err_ctx("Failed to update report", "Database error")?;

    tx.commit().await.db_err_ctx("Failed to commit transaction", "Database error")?;

    for key in &media_keys {
        delete_media(&state, key).await;
    }

    tracing::info!("Report {} actioned (content removed) by admin {}", report_id, auth.user_id);
    Ok(StatusCode::NO_CONTENT)
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
