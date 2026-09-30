//! In-app feedback (bug reports, ideas) for the friend-and-family test.
//!
//! Any signed-in user can send feedback; admins read and triage it at
//! /admin/feedback. Technical context (page, browser, screen size) is
//! optional and shown to the sender before sending. Up to three
//! screenshots can be attached; they're served only through the admin API
//! and kept much shorter than the text, since they often show other
//! people's posts or messages (see the migration and `spawn_cleanup`).
//! Feedback itself is deleted a year after it was sent.

use std::time::Duration;

use axum::{
    extract::{Multipart, Path, Query, State},
    http::{header, StatusCode},
    response::IntoResponse,
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::errors::AppError;
use crate::handlers::auth::AppState;
use crate::handlers::reports::require_admin;
use crate::media;
use crate::utils::DbResultExt;

const MESSAGE_MIN: usize = 5;
const MESSAGE_MAX: usize = 4000;
const NOTE_MAX: usize = 1000;
/// Per-user cap, on top of the general per-IP rate limit, so one account
/// can't flood the admin view.
const MAX_PER_DAY: i64 = 20;
const RETENTION_DAYS: i32 = 365;
pub const MAX_SCREENSHOTS: usize = 3;
pub const MAX_SCREENSHOT_BYTES: usize = 10 * 1024 * 1024;
const ALLOWED_TYPES: &[&str] = &["image/jpeg", "image/png", "image/webp"];
/// Screenshots go this long after the feedback is marked done...
const SCREENSHOT_DONE_DAYS: i32 = 30;
/// ...and this long after sending at the latest.
const SCREENSHOT_MAX_DAYS: i32 = 90;

/// A processed screenshot, ready to store.
struct Screenshot {
    webp: Vec<u8>,
    width: u32,
    height: u32,
}

/// The text fields of the feedback form.
#[derive(Debug, Default)]
struct CreateFeedbackRequest {
    category: String,
    message: String,
    /// Technical context, sent only if the user leaves it on.
    page_path: Option<String>,
    user_agent: Option<String>,
    viewport: Option<String>,
}

/// Trims an optional context field and caps its length; these come from
/// the browser, so they're treated as untrusted text.
fn context(value: Option<String>, max: usize) -> Option<String> {
    value
        .map(|v| v.trim().chars().take(max).collect::<String>())
        .filter(|v| !v.is_empty())
}

/// Reads the multipart form: text fields plus up to MAX_SCREENSHOTS
/// "screenshot" files, each decoded and re-encoded right away so nothing
/// unprocessed is ever stored.
async fn read_form(mut multipart: Multipart) -> Result<(CreateFeedbackRequest, Vec<Screenshot>), AppError> {
    let mut input = CreateFeedbackRequest::default();
    let mut screenshots = Vec::new();
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::bad_request(format!("Invalid multipart data: {}", e)))?
    {
        let name = field.name().unwrap_or("").to_string();
        if name == "screenshot" {
            if screenshots.len() == MAX_SCREENSHOTS {
                return Err(AppError::bad_request(format!("At most {} screenshots", MAX_SCREENSHOTS)));
            }
            if !field.content_type().is_some_and(|ct| ALLOWED_TYPES.contains(&ct)) {
                return Err(AppError::bad_request("Screenshots must be JPEG, PNG or WebP"));
            }
            let bytes = field
                .bytes()
                .await
                .map_err(|e| AppError::bad_request(format!("Failed to read screenshot: {}", e)))?;
            if bytes.len() > MAX_SCREENSHOT_BYTES {
                return Err(AppError::bad_request("Each screenshot must be under 10 MB"));
            }
            let (webp, width, height) = tokio::task::spawn_blocking(move || media::process_screenshot(&bytes))
                .await
                .map_err(|e| {
                    tracing::error!("Screenshot processing task failed: {}", e);
                    AppError::internal("Image processing failed")
                })?
                .map_err(|e| AppError::bad_request(format!("Screenshot could not be read: {}", e)))?;
            screenshots.push(Screenshot { webp, width, height });
            continue;
        }
        let text = field
            .text()
            .await
            .map_err(|e| AppError::bad_request(format!("Invalid field {}: {}", name, e)))?;
        match name.as_str() {
            "category" => input.category = text,
            "message" => input.message = text,
            "page_path" => input.page_path = Some(text),
            "user_agent" => input.user_agent = Some(text),
            "viewport" => input.viewport = Some(text),
            _ => {}
        }
    }
    Ok((input, screenshots))
}

/// POST /feedback (auth required) -- multipart/form-data.
pub async fn create_feedback(
    State(state): State<AppState>,
    auth: AuthUser,
    multipart: Multipart,
) -> Result<StatusCode, AppError> {
    let (input, screenshots) = read_form(multipart).await?;
    if !matches!(input.category.as_str(), "bug" | "idea" | "other") {
        return Err(AppError::bad_request("Invalid category"));
    }
    let message = input.message.trim();
    let len = message.chars().count();
    if len < MESSAGE_MIN {
        return Err(AppError::bad_request("Please write a little more"));
    }
    if len > MESSAGE_MAX {
        return Err(AppError::bad_request(format!("Feedback must be under {} characters", MESSAGE_MAX)));
    }

    let today = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM feedback WHERE user_id = $1 AND created_at > NOW() - INTERVAL '1 day'",
    )
    .bind(auth.user_id)
    .fetch_one(&state.db)
    .await
    .db_err("Database error")?;
    if today >= MAX_PER_DAY {
        return Err(AppError::bad_request("That's a lot of feedback for one day — thank you! Please try again tomorrow."));
    }

    // Only the path: a query string could carry tokens (e.g. a reset link).
    let page_path = context(input.page_path, 200).map(|p| p.split(['?', '#']).next().unwrap_or("").to_string());

    // Files first, then the rows in one transaction; if anything fails the
    // files already written are removed again, so no screenshot is left in
    // storage without a row that its cleanup could find.
    let keys: Vec<String> = screenshots.iter().map(|_| format!("feedback/{}.webp", Uuid::new_v4())).collect();
    let mut saved = Vec::new();
    let result = async {
        for (key, shot) in keys.iter().zip(&screenshots) {
            state.storage.save(key, &shot.webp).await?;
            saved.push(key.as_str());
        }
        let mut tx = state.db.begin().await.db_err("Database error")?;
        let feedback_id = sqlx::query_scalar::<_, Uuid>(
            r#"
            INSERT INTO feedback (user_id, category, message, page_path, user_agent, viewport)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING id
            "#,
        )
        .bind(auth.user_id)
        .bind(&input.category)
        .bind(message)
        .bind(page_path)
        .bind(context(input.user_agent, 300))
        .bind(context(input.viewport, 20))
        .fetch_one(&mut *tx)
        .await
        .db_err_ctx("Failed to save feedback", "Database error")?;
        for (i, (key, shot)) in keys.iter().zip(&screenshots).enumerate() {
            sqlx::query(
                "INSERT INTO feedback_screenshots (feedback_id, storage_key, width, height, sort_order) VALUES ($1, $2, $3, $4, $5)",
            )
            .bind(feedback_id)
            .bind(key)
            .bind(shot.width as i32)
            .bind(shot.height as i32)
            .bind(i as i32)
            .execute(&mut *tx)
            .await
            .db_err_ctx("Failed to save feedback screenshot", "Database error")?;
        }
        tx.commit().await.db_err("Database error")
    }
    .await;
    if let Err(e) = result {
        for key in saved {
            delete_file(&state, key).await;
        }
        return Err(e);
    }

    tracing::info!("Feedback received ({}, {} screenshots)", input.category, screenshots.len());
    Ok(StatusCode::CREATED)
}

/// Screenshots are never on the CDN (only served by
/// get_screenshot), so there's nothing to purge -- just the file.
async fn delete_file(state: &AppState, key: &str) {
    if let Err(e) = state.storage.delete(key).await {
        tracing::error!("Failed to delete feedback screenshot {}: {}", key, e.message);
    }
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ScreenshotInfo {
    pub id: Uuid,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct FeedbackRow {
    pub id: Uuid,
    /// None once the sender's account is deleted.
    pub username: Option<String>,
    pub category: String,
    pub message: String,
    pub page_path: Option<String>,
    pub user_agent: Option<String>,
    pub viewport: Option<String>,
    pub status: String,
    pub admin_note: Option<String>,
    pub created_at: DateTime<Utc>,
    /// Loaded separately; the images themselves come from get_screenshot.
    #[sqlx(skip)]
    pub screenshots: Vec<ScreenshotInfo>,
}

#[derive(Debug, Deserialize)]
pub struct FeedbackQuery {
    /// "open" (new + seen, the default) or "all".
    pub filter: Option<String>,
}

/// GET /admin/feedback (admin only) -- newest first.
pub async fn list_feedback(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<FeedbackQuery>,
) -> Result<Json<Vec<FeedbackRow>>, AppError> {
    require_admin(&state.db, &auth).await?;
    let all = query.filter.as_deref() == Some("all");

    let mut rows = sqlx::query_as::<_, FeedbackRow>(
        r#"
        SELECT f.id, u.username, f.category, f.message, f.page_path, f.user_agent, f.viewport,
               f.status, f.admin_note, f.created_at
        FROM feedback f LEFT JOIN users u ON u.id = f.user_id
        WHERE $1 OR f.status != 'done'
        ORDER BY f.created_at DESC
        LIMIT 500
        "#,
    )
    .bind(all)
    .fetch_all(&state.db)
    .await
    .db_err("Database error")?;

    let ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();
    let shots = sqlx::query_as::<_, (Uuid, Uuid, i32, i32)>(
        "SELECT feedback_id, id, width, height FROM feedback_screenshots WHERE feedback_id = ANY($1) ORDER BY sort_order",
    )
    .bind(&ids)
    .fetch_all(&state.db)
    .await
    .db_err("Database error")?;
    for (feedback_id, id, width, height) in shots {
        if let Some(row) = rows.iter_mut().find(|r| r.id == feedback_id) {
            row.screenshots.push(ScreenshotInfo { id, width, height });
        }
    }
    Ok(Json(rows))
}

/// GET /admin/feedback/screenshots/:id (admin only) -- the image itself.
/// Served from here rather than a CDN link so it never exists at a URL
/// that works without admin rights, and so each view is logged.
pub async fn get_screenshot(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(screenshot_id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    require_admin(&state.db, &auth).await?;
    let key = sqlx::query_scalar::<_, String>("SELECT storage_key FROM feedback_screenshots WHERE id = $1")
        .bind(screenshot_id)
        .fetch_optional(&state.db)
        .await
        .db_err("Database error")?
        .ok_or_else(|| AppError::not_found("Screenshot not found"))?;
    let bytes = state.storage.get(&key).await?;
    tracing::info!("Admin {} viewed feedback screenshot {}", auth.user_id, screenshot_id);
    Ok((
        [
            (header::CONTENT_TYPE, "image/webp"),
            (header::CACHE_CONTROL, "private, no-store"),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        ],
        bytes,
    ))
}

#[derive(Debug, Deserialize)]
pub struct UpdateFeedbackRequest {
    pub status: String,
    pub admin_note: Option<String>,
}

/// PATCH /admin/feedback/:id (admin only) -- triage: status and an
/// internal note (e.g. a link to the issue it became).
pub async fn update_feedback(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(feedback_id): Path<Uuid>,
    Json(input): Json<UpdateFeedbackRequest>,
) -> Result<StatusCode, AppError> {
    require_admin(&state.db, &auth).await?;
    if !matches!(input.status.as_str(), "new" | "seen" | "done") {
        return Err(AppError::bad_request("Invalid status"));
    }
    let note = input.admin_note.map(|n| n.trim().to_string()).filter(|n| !n.is_empty());
    if note.as_ref().is_some_and(|n| n.chars().count() > NOTE_MAX) {
        return Err(AppError::bad_request(format!("Note must be under {} characters", NOTE_MAX)));
    }

    // done_at starts the screenshots' deletion clock; reopening stops it.
    let updated = sqlx::query(
        "UPDATE feedback SET status = $2, admin_note = $3, \
         done_at = CASE WHEN $2 = 'done' THEN COALESCE(done_at, NOW()) END WHERE id = $1",
    )
        .bind(feedback_id)
        .bind(&input.status)
        .bind(note)
        .execute(&state.db)
        .await
        .db_err("Database error")?
        .rows_affected();
    if updated == 0 {
        return Err(AppError::not_found("Feedback not found"));
    }
    Ok(StatusCode::NO_CONTENT)
}

/// For the data export (Art. 15): the feedback this user sent, plus
/// (path in the ZIP, storage key) of each screenshot still kept.
pub async fn export_for(db: &sqlx::PgPool, user_id: Uuid) -> Result<(Vec<serde_json::Value>, Vec<(String, String)>), AppError> {
    let rows = sqlx::query_as::<_, (Uuid, serde_json::Value)>(
        r#"
        SELECT id, jsonb_build_object('category', category, 'message', message, 'page_path', page_path,
            'user_agent', user_agent, 'viewport', viewport, 'status', status, 'created_at', created_at)
        FROM feedback WHERE user_id = $1 ORDER BY created_at
        "#,
    )
    .bind(user_id)
    .fetch_all(db)
    .await
    .db_err_ctx("Data export query failed", "Database error")?;
    let shots = sqlx::query_as::<_, (Uuid, String, i32)>(
        r#"
        SELECT s.feedback_id, s.storage_key, s.sort_order
        FROM feedback_screenshots s JOIN feedback f ON f.id = s.feedback_id
        WHERE f.user_id = $1 ORDER BY s.sort_order
        "#,
    )
    .bind(user_id)
    .fetch_all(db)
    .await
    .db_err_ctx("Data export query failed", "Database error")?;

    let mut files = Vec::new();
    let entries = rows
        .into_iter()
        .map(|(id, mut entry)| {
            let paths: Vec<String> = shots
                .iter()
                .filter(|s| s.0 == id)
                .map(|(_, key, n)| {
                    let path = format!("feedback/{id}/{n}.webp");
                    files.push((path.clone(), key.clone()));
                    path
                })
                .collect();
            entry["screenshots"] = serde_json::json!(paths);
            entry
        })
        .collect();
    Ok((entries, files))
}

/// Removes the screenshot rows of everything this user sent, inside the
/// account-deletion transaction, and returns their storage keys so the
/// files can go once it commits. The feedback text itself stays, unlinked.
pub async fn forget_screenshots(tx: &mut sqlx::PgConnection, user_id: Uuid) -> Result<Vec<String>, AppError> {
    sqlx::query_scalar::<_, String>(
        r#"
        DELETE FROM feedback_screenshots s USING feedback f
        WHERE s.feedback_id = f.id AND f.user_id = $1
        RETURNING s.storage_key
        "#,
    )
    .bind(user_id)
    .fetch_all(tx)
    .await
    .db_err_ctx("Failed to delete feedback screenshots", "Failed to delete account")
}

/// Once a day on every replica: screenshots past their retention, then
/// feedback older than a year. Plain DELETEs, so replicas running it at
/// the same time is harmless; a replica only removes the files of the
/// rows its own DELETE returned.
pub fn spawn_cleanup(state: AppState) {
    tokio::spawn(async move {
        loop {
            match delete_expired_screenshots(&state).await {
                Ok(n) if n > 0 => tracing::info!("Deleted {} feedback screenshots past retention", n),
                Ok(_) => {}
                Err(e) => tracing::error!("Feedback screenshot cleanup failed: {}", e),
            }
            match delete_expired(&state.db).await {
                Ok(n) if n > 0 => tracing::info!("Deleted {} feedback entries past retention", n),
                Ok(_) => {}
                Err(e) => tracing::error!("Feedback cleanup failed: {}", e),
            }
            tokio::time::sleep(Duration::from_secs(24 * 60 * 60)).await;
        }
    });
}

/// One screenshot cleanup pass; returns how many went.
pub(crate) async fn delete_expired_screenshots(state: &AppState) -> Result<usize, sqlx::Error> {
    let keys = sqlx::query_scalar::<_, String>(
        r#"
        DELETE FROM feedback_screenshots s USING feedback f
        WHERE s.feedback_id = f.id
          AND (f.done_at < NOW() - make_interval(days => $1)
               OR s.created_at < NOW() - make_interval(days => $2))
        RETURNING s.storage_key
        "#,
    )
    .bind(SCREENSHOT_DONE_DAYS)
    .bind(SCREENSHOT_MAX_DAYS)
    .fetch_all(&state.db)
    .await?;
    for key in &keys {
        delete_file(state, key).await;
    }
    Ok(keys.len())
}

/// One cleanup pass; returns how many rows went.
pub(crate) async fn delete_expired(db: &sqlx::PgPool) -> Result<u64, sqlx::Error> {
    sqlx::query("DELETE FROM feedback WHERE created_at < NOW() - make_interval(days => $1)")
        .bind(RETENTION_DAYS)
        .execute(db)
        .await
        .map(|r| r.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_trims_caps_and_drops_empty() {
        assert_eq!(context(None, 10), None);
        assert_eq!(context(Some("   ".into()), 10), None);
        assert_eq!(context(Some(" abc ".into()), 10).as_deref(), Some("abc"));
        assert_eq!(context(Some("abcdef".into()), 3).as_deref(), Some("abc"));
    }
}
