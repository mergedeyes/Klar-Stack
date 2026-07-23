use axum::{
    extract::State,
    response::sse::{Event, Sse},
    Json,
};
use futures::stream::Stream;
use redis::AsyncCommands;
use serde::Serialize;
use std::convert::Infallible;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::errors::AppError;
use crate::handlers::auth::AppState;
use crate::models::UserResponse;

/// Redis pub/sub channel that all backend replicas subscribe to for
/// fanning real-time notifications out across the cluster. See main.rs
/// for the subscriber task that forwards messages on this channel into
/// each replica's local broadcast::channel (which the SSE handler below
/// actually reads from).
pub const NOTIFICATION_CHANNEL: &str = "klar:notifications";

#[derive(Clone, Debug, Serialize, serde::Deserialize)]
pub struct NotificationEvent {
    pub target_user_id: Uuid,
    pub notification: NotificationResponse,
}

#[derive(Debug, Serialize, Clone, serde::Deserialize)]
pub struct NotificationResponse {
    pub id: Uuid,
    pub type_name: String,
    pub is_read: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub actor: UserResponse,
    pub post_id: Option<Uuid>,
}

/// Publish a notification event to Redis so every backend replica (not
/// just the one handling this request) can deliver it to any matching SSE
/// subscriber it holds. Errors are logged, not propagated — a failed
/// real-time push shouldn't fail the underlying action (e.g. a like),
/// since the notification row is already durably stored in Postgres and
/// will show up next time the client polls GET /notifications.
pub async fn publish_notification(state: &AppState, event: &NotificationEvent) {
    let payload = match serde_json::to_string(event) {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("Failed to serialize notification event: {}", e);
            return;
        }
    };

    let mut conn = state.redis.clone();
    if let Err(e) = conn.publish::<_, _, ()>(NOTIFICATION_CHANNEL, payload).await {
        tracing::error!("Failed to publish notification to Redis: {}", e);
    }
}

/// GET /notifications — Fetch historical notifications
pub async fn get_notifications(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Vec<NotificationResponse>>, AppError> {
    
    let records = sqlx::query!(
        r#"
        SELECT 
            n.id, n.type::text as type_name, n.is_read, n.created_at, n.post_id,
            u.id as actor_id, u.username as actor_username, u.email as actor_email, 
            u.display_name as actor_display, u.bio as actor_bio, 
            u.avatar_url as actor_avatar, u.email_verified, u.created_at as actor_created,
            u.username_changed_at as actor_username_changed_at
        FROM notifications n
        JOIN users u ON n.actor_id = u.id
        WHERE n.user_id = $1
        ORDER BY n.created_at DESC
        LIMIT 50
        "#,
        auth.user_id
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Database error: {}", e);
        AppError::internal("Database error")
    })?;

    let responses = records.into_iter().map(|rec| NotificationResponse {
        id: rec.id,
        type_name: rec.type_name.unwrap_or_default(),
        is_read: rec.is_read,
        created_at: rec.created_at,
        post_id: rec.post_id,
        actor: UserResponse {
            id: rec.actor_id,
            username: rec.actor_username,
            email: rec.actor_email,
            display_name: rec.actor_display,
            bio: rec.actor_bio,
            avatar_url: rec.actor_avatar,
            email_verified: rec.email_verified,
            created_at: rec.actor_created,
            username_changed_at: rec.actor_username_changed_at,
        },
    }).collect();

    Ok(Json(responses))
}

/// GET /notifications/stream — SSE Endpoint
///
/// Reads from the *local* in-process broadcast channel only. Cross-replica
/// delivery happens upstream: publish_notification() PUBLISHes to Redis,
/// and the subscriber task spawned in main.rs re-broadcasts every message
/// it receives from Redis into this same local channel on every replica —
/// including the replica that originally published it. So this handler
/// doesn't need to know or care about Redis at all.
pub async fn notification_stream(
    State(state): State<AppState>,
    auth: AuthUser, 
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, AppError> {
    
    let user_id = auth.user_id;
    let mut rx = state.notification_tx.subscribe();

    let stream = async_stream::stream! {
        while let Ok(event) = rx.recv().await {
            if event.target_user_id == user_id {
                if let Ok(json) = serde_json::to_string(&event.notification) {
                    yield Ok(Event::default().data(json));
                }
            }
        }
    };

    Ok(Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::new()))
}

/// PATCH /notifications/read — Mark all as read
pub async fn mark_read(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, AppError> {
    sqlx::query!("UPDATE notifications SET is_read = TRUE WHERE user_id = $1", auth.user_id)
        .execute(&state.db)
        .await
        .map_err(|_| AppError::internal("Failed to update notifications"))?;
        
    Ok(Json(serde_json::json!({"message": "ok"})))
}
