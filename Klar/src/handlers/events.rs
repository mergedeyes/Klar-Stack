//! The interaction log for ranking the Discovery page (post_events, see
//! migrations/20261002100000_post_events.sql). The home feed stays
//! chronological and never reads it.
//!
//! `record_event` is called by the handlers where the interaction already
//! happens (likes, comments, comment likes); nothing extra is sent by the
//! browser. Each account can switch it off (`PATCH
//! /users/me/personalization`), which also deletes what was logged.

use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::errors::AppError;
use crate::handlers::auth::AppState;
use crate::utils::DbResultExt;

/// Mirrors the CHECK constraint on post_events.event_type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventType {
    Like,
    Unlike,
    Comment,
    CommentLike,
    CommentUnlike,
}

impl EventType {
    pub fn as_str(&self) -> &'static str {
        match self {
            EventType::Like => "like",
            EventType::Unlike => "unlike",
            EventType::Comment => "comment",
            EventType::CommentLike => "comment_like",
            EventType::CommentUnlike => "comment_unlike",
        }
    }
}

/// Logs one interaction, unless the account switched personalisation off.
/// Called after the interaction committed, and never fails it: the like
/// is what counts, its log entry is secondary, so an error is only logged.
///
/// FOR SHARE waits for an opt-out in progress and then re-reads the flag,
/// so an event can't slip in after the opt-out deleted the account's log.
pub async fn record_event(pool: &PgPool, user_id: Uuid, post_id: Uuid, event_type: EventType) {
    if let Err(e) = sqlx::query(
        r#"
        INSERT INTO post_events (user_id, post_id, event_type)
        SELECT id, $2, $3 FROM users WHERE id = $1 AND personalization_enabled
        FOR SHARE
        "#,
    )
    .bind(user_id)
    .bind(post_id)
    .bind(event_type.as_str())
    .execute(pool)
    .await
    {
        tracing::warn!("Failed to record {} event for post {}: {}", event_type.as_str(), post_id, e);
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Personalization {
    pub enabled: bool,
}

/// PATCH /users/me/personalization — switch the interaction log on or off.
/// Switching it off is the objection to the processing (Art. 21 GDPR):
/// the log is deleted in the same transaction, and nothing more is
/// recorded until it is switched on again. Allowed while suspended, like
/// every other privacy right (standing.rs).
pub async fn set_personalization(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<Personalization>,
) -> Result<Json<Personalization>, AppError> {
    let mut tx = state.db.begin().await.db_err("Database error")?;
    sqlx::query("UPDATE users SET personalization_enabled = $2 WHERE id = $1")
        .bind(auth.user_id)
        .bind(input.enabled)
        .execute(&mut *tx)
        .await
        .db_err_ctx("Failed to update personalization", "Database error")?;
    if !input.enabled {
        let deleted = sqlx::query("DELETE FROM post_events WHERE user_id = $1")
            .bind(auth.user_id)
            .execute(&mut *tx)
            .await
            .db_err_ctx("Failed to delete post events", "Database error")?
            .rows_affected();
        tracing::info!("Personalization switched off for {}: {} event(s) deleted", auth.user_id, deleted);
    }
    tx.commit().await.db_err("Database error")?;
    Ok(Json(input))
}
