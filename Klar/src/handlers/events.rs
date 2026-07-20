/// Interaction event log handlers.
///
/// `record_event` is called directly from other handlers (likes,
/// comments) right where the action already happens server-side --
/// no extra request needed for those. The one thing the server can't
/// observe on its own is whether a post was actually seen, so `POST
/// /events` exists for the frontend to report view/impression events.

use axum::{extract::State, http::StatusCode, Json};
use sqlx::PgPool;
use uuid::Uuid;

use crate::auth::OptionalAuthUser;
use crate::errors::AppError;
use crate::handlers::auth::AppState;
use crate::models::{EventType, RecordEventRequest};

/// Insert one row into post_events. Fire-and-forget by design: a failed
/// event write should never fail the request that triggered it (a like
/// succeeding is what matters; the analytics record of that like is
/// secondary), so callers should log a warning on error and move on
/// rather than propagate an AppError.
pub async fn record_event(
    pool: &PgPool,
    user_id: Option<Uuid>,
    post_id: Uuid,
    event_type: EventType,
) {
    if let Err(e) = sqlx::query(
        "INSERT INTO post_events (user_id, post_id, event_type) VALUES ($1, $2, $3)",
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

/// POST /events — client-reported events (currently just post views).
/// Optionally authenticated: logged-out views are still worth recording
/// for later ranking/analytics work, just without a user_id attached.
pub async fn create_event(
    State(state): State<AppState>,
    auth: OptionalAuthUser,
    Json(input): Json<RecordEventRequest>,
) -> Result<StatusCode, AppError> {
    let event_type = match input.event_type.as_str() {
        "view" => EventType::View,
        other => return Err(AppError::bad_request(format!("Unsupported client-reported event type: {}", other))),
    };

    record_event(&state.db, auth.user_id, input.post_id, event_type).await;

    Ok(StatusCode::NO_CONTENT)
}
