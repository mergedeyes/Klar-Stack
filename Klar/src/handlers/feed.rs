use axum::{
    extract::{Query, State},
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::OptionalAuthUser;
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

/// GET /feed/discovery — public, cross-user discovery feed.
///
/// Needed OptionalAuthUser added here for one reason: without it, this
/// had zero privacy filtering at all -- every private account's posts
/// were fully visible to anyone, logged in or not, defeating the entire
/// point of the private-account feature. The added clause excludes a
/// private account's posts unless the viewer is the owner or an accepted
/// follower (binding None for an anonymous viewer correctly excludes all
/// private accounts, since `u.id = NULL` and the follows EXISTS subquery
/// both evaluate to false/no-match).
///
/// "Hidden" posts (auto-hidden via a CSAM report) are excluded outright,
/// even for their own author -- this is a discovery surface, not "my own
/// content" view, so there's no case here where showing it to anyone
/// (owner included) makes sense while it's hidden.
pub async fn get_global_feed(
    State(state): State<AppState>,
    auth: OptionalAuthUser,
    Query(params): Query<FeedQuery>,
) -> Result<Json<FeedResponse>, AppError> {
    
    // Wir cappen das Limit serverseitig auf maximal 50, um Missbrauch zu verhindern
    let limit = params.limit.unwrap_or(20).clamp(1, 50);
    let viewer_id = auth.user_id;

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
                    p.like_count,
                    p.moderation_status::text
                FROM posts p
                JOIN users u ON p.user_id = u.id
                LEFT JOIN media_assets m ON m.post_id = p.id AND m.sort_order = 0
                WHERE (p.created_at, p.id) < ($1, $2)
                  AND p.moderation_status != 'hidden'
                  AND (
                    u.is_private = FALSE
                    OR u.id = $4
                    OR EXISTS(SELECT 1 FROM follows f WHERE f.follower_id = $4 AND f.following_id = u.id)
                  )
                ORDER BY p.created_at DESC, p.id DESC
                LIMIT $3
                "#
            )
            .bind(c_time)
            .bind(c_id)
            .bind(limit)
            .bind(viewer_id)
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
                    p.like_count,
                    p.moderation_status::text
                FROM posts p
                JOIN users u ON p.user_id = u.id
                LEFT JOIN media_assets m ON m.post_id = p.id AND m.sort_order = 0
                WHERE p.moderation_status != 'hidden'
                  AND (
                    u.is_private = FALSE
                    OR u.id = $2
                    OR EXISTS(SELECT 1 FROM follows f WHERE f.follower_id = $2 AND f.following_id = u.id)
                  )
                ORDER BY p.created_at DESC, p.id DESC
                LIMIT $1
                "#
            )
            .bind(limit)
            .bind(viewer_id)
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
