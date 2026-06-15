use axum::{
    extract::State,
    response::sse::{Event, Sse},
    Json,
};
use futures::stream::Stream;
use serde::Serialize;
use std::convert::Infallible;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::errors::AppError;
use crate::handlers::auth::AppState;
use crate::models::UserResponse;

#[derive(Clone, Debug, Serialize)]
pub struct NotificationEvent {
    pub target_user_id: Uuid,
    pub notification: NotificationResponse,
}

#[derive(Debug, Serialize, Clone)]
pub struct NotificationResponse {
    pub id: Uuid,
    pub type_name: String,
    pub is_read: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub actor: UserResponse,
    pub post_id: Option<Uuid>,
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