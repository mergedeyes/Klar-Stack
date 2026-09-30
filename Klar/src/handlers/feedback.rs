//! In-app feedback (bug reports, ideas) for the friend-and-family test.
//!
//! Any signed-in user can send feedback; admins read and triage it at
//! /admin/feedback. Technical context (page, browser, screen size) is
//! optional and shown to the sender before sending. Feedback is deleted a
//! year after it was sent by `spawn_cleanup`.

use std::time::Duration;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::errors::AppError;
use crate::handlers::auth::AppState;
use crate::handlers::reports::require_admin;
use crate::utils::DbResultExt;

const MESSAGE_MIN: usize = 5;
const MESSAGE_MAX: usize = 4000;
const NOTE_MAX: usize = 1000;
/// Per-user cap, on top of the general per-IP rate limit, so one account
/// can't flood the admin view.
const MAX_PER_DAY: i64 = 20;
const RETENTION_DAYS: i32 = 365;

#[derive(Debug, Deserialize)]
pub struct CreateFeedbackRequest {
    pub category: String,
    pub message: String,
    /// Technical context, sent only if the user leaves it on.
    pub page_path: Option<String>,
    pub user_agent: Option<String>,
    pub viewport: Option<String>,
}

/// Trims an optional context field and caps its length; these come from
/// the browser, so they're treated as untrusted text.
fn context(value: Option<String>, max: usize) -> Option<String> {
    value
        .map(|v| v.trim().chars().take(max).collect::<String>())
        .filter(|v| !v.is_empty())
}

/// POST /feedback (auth required)
pub async fn create_feedback(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<CreateFeedbackRequest>,
) -> Result<StatusCode, AppError> {
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

    sqlx::query(
        r#"
        INSERT INTO feedback (user_id, category, message, page_path, user_agent, viewport)
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
    )
    .bind(auth.user_id)
    .bind(&input.category)
    .bind(message)
    .bind(page_path)
    .bind(context(input.user_agent, 300))
    .bind(context(input.viewport, 20))
    .execute(&state.db)
    .await
    .db_err_ctx("Failed to save feedback", "Database error")?;

    tracing::info!("Feedback received ({})", input.category);
    Ok(StatusCode::CREATED)
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

    let rows = sqlx::query_as::<_, FeedbackRow>(
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
    Ok(Json(rows))
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

    let updated = sqlx::query("UPDATE feedback SET status = $2, admin_note = $3 WHERE id = $1")
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

/// For the data export (Art. 15): the feedback this user sent.
pub async fn export_for(db: &sqlx::PgPool, user_id: Uuid) -> Result<Vec<serde_json::Value>, AppError> {
    sqlx::query_scalar::<_, serde_json::Value>(
        r#"
        SELECT jsonb_build_object('category', category, 'message', message, 'page_path', page_path,
            'user_agent', user_agent, 'viewport', viewport, 'status', status, 'created_at', created_at)
        FROM feedback WHERE user_id = $1 ORDER BY created_at
        "#,
    )
    .bind(user_id)
    .fetch_all(db)
    .await
    .db_err_ctx("Data export query failed", "Database error")
}

/// Deletes feedback older than a year, once a day on every replica (a
/// plain DELETE, so replicas running it concurrently is harmless).
pub fn spawn_cleanup(db: sqlx::PgPool) {
    tokio::spawn(async move {
        loop {
            match delete_expired(&db).await {
                Ok(n) if n > 0 => tracing::info!("Deleted {} feedback entries past retention", n),
                Ok(_) => {}
                Err(e) => tracing::error!("Feedback cleanup failed: {}", e),
            }
            tokio::time::sleep(Duration::from_secs(24 * 60 * 60)).await;
        }
    });
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
