use axum::{
    extract::{Query, State},
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::errors::AppError;
use crate::handlers::auth::AppState;
use crate::models::post::PostResponse;

#[derive(Deserialize)]
pub struct FeedQuery {
    /// Wie viele Posts sollen geladen werden? (Default z.B. 20)
    pub limit: Option<i64>,
    /// Der Zeitstempel des letzten Posts der aktuellen Ansicht
    pub cursor_time: Option<DateTime<Utc>>,
    /// Die ID des letzten Posts (als Tie-Breaker)
    pub cursor_id: Option<Uuid>,
}

#[derive(Serialize)]
pub struct FeedResponse {
    pub data: Vec<PostResponse>,
    /// Wenn dieser Wert null ist, ist der User am Ende des Feeds angelangt
    pub next_cursor: Option<CursorData>,
}

#[derive(Serialize, Clone)]
pub struct CursorData {
    pub time: DateTime<Utc>,
    pub id: Uuid,
}

pub async fn get_global_feed(
    State(state): State<AppState>,
    Query(params): Query<FeedQuery>,
) -> Result<Json<FeedResponse>, AppError> {
    
    // Wir cappen das Limit serverseitig auf maximal 50, um Missbrauch zu verhindern
    let limit = params.limit.unwrap_or(20).clamp(1, 50);

    // WICHTIG für Performance (Keyset Pagination): 
    // Wir splitten die Query in zwei Pfade, um den perfekten "Index Scan" zu garantieren.
    
    let query_result = match (params.cursor_time, params.cursor_id) {
        (Some(c_time), Some(c_id)) => {
            // PFAD 1: Der User scrollt (Nachladen mit Cursor)
            // Nutzt den Composite Index (created_at DESC, id DESC) extrem effizient (O(1))
            sqlx::query_as::<_, PostResponse>(
                r#"
                SELECT
                    p.id,
                    p.user_id,
                    u.username,
                    u.avatar_url,
                    p.caption,
                    p.created_at,
                    p.edited_at,
                    m.thumb_key AS thumb_url,
                    m.medium_key AS medium_url,
                    m.full_key AS full_url,
                    p.comment_count,
                    p.like_count
                FROM posts p
                JOIN users u ON p.user_id = u.id
                LEFT JOIN media_assets m ON m.post_id = p.id AND m.sort_order = 0
                WHERE (p.created_at, p.id) < ($1, $2)
                ORDER BY p.created_at DESC, p.id DESC
                LIMIT $3
                "#
            )
            .bind(c_time)
            .bind(c_id)
            .bind(limit)
            .fetch_all(&state.db)
            .await
        }
        _ => {
            // PFAD 2: Erster Aufruf (Start des Feeds)
            sqlx::query_as::<_, PostResponse>(
                r#"
                SELECT
                    p.id,
                    p.user_id,
                    u.username,
                    u.avatar_url,
                    p.caption,
                    p.created_at,
                    p.edited_at,
                    m.thumb_key AS thumb_url,
                    m.medium_key AS medium_url,
                    m.full_key AS full_url,
                    p.comment_count,
                    p.like_count
                FROM posts p
                JOIN users u ON p.user_id = u.id
                LEFT JOIN media_assets m ON m.post_id = p.id AND m.sort_order = 0
                ORDER BY p.created_at DESC, p.id DESC
                LIMIT $1
                "#
            )
            .bind(limit)
            .fetch_all(&state.db)
            .await
        }
    };

    let posts = query_result.map_err(|e| {
        tracing::error!("Failed to fetch discovery feed: {:?}", e);
        AppError::internal("Database error")
    })?;

    // Wir nehmen das letzte Element aus der Datenbank-Antwort und bauen den Cursor für den nächsten Request
    let next_cursor = posts.last().map(|last_post| CursorData {
        time: last_post.created_at,
        id: last_post.id,
    });

    Ok(Json(FeedResponse {
        data: posts,
        next_cursor,
    }))
}
