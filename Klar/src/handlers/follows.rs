/// Follow/unfollow handlers and follower/following lists.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Serialize;

use crate::auth::AuthUser;
use crate::errors::AppError;
use crate::handlers::auth::AppState;
use crate::handlers::blocks::check_block;
use crate::models::UserResponse;

/// Response for follow/unfollow actions
#[derive(Serialize)]
pub struct FollowResponse {
    pub message: String,
}

/// Profile stats — returned with user profiles
#[derive(Serialize)]
pub struct ProfileStats {
    pub followers: i64,
    pub following: i64,
    pub posts: i64,
}

/// POST /users/:username/follow — follow a user (auth required)
pub async fn follow_user(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(username): Path<String>,
) -> Result<(StatusCode, Json<FollowResponse>), AppError> {

    // Look up the target user
    let target = sqlx::query_scalar::<_, uuid::Uuid>(
        "SELECT id FROM users WHERE username = $1"
    )
    .bind(&username)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Database error: {}", e);
        AppError::internal("Database error")
    })?
    .ok_or_else(|| AppError::not_found(format!("User '{}' not found", username)))?;

    // Can't follow yourself (DB constraint will catch this too, but better UX to check early)
    if target == auth.user_id {
        return Err(AppError::bad_request("You can't follow yourself"));
    }

    // Can't follow if either party has blocked the other
    if check_block(&state.db, auth.user_id, target).await? {
        return Err(AppError::bad_request("Cannot follow this user"));
    }

    // Insert follow — ON CONFLICT DO NOTHING handles the "already following" case
    sqlx::query(
        "INSERT INTO follows (follower_id, following_id) VALUES ($1, $2) ON CONFLICT DO NOTHING"
    )
    .bind(auth.user_id)
    .bind(target)
    .execute(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Failed to follow: {}", e);
        AppError::internal("Failed to follow user")
    })?;

    tracing::info!("User {} followed {}", auth.user_id, username);
    Ok((
        StatusCode::CREATED,
        Json(FollowResponse {
            message: format!("Now following {}", username),
        }),
    ))
}

/// DELETE /users/:username/follow — unfollow a user (auth required)
pub async fn unfollow_user(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(username): Path<String>,
) -> Result<Json<FollowResponse>, AppError> {

    let target = sqlx::query_scalar::<_, uuid::Uuid>(
        "SELECT id FROM users WHERE username = $1"
    )
    .bind(&username)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Database error: {}", e);
        AppError::internal("Database error")
    })?
    .ok_or_else(|| AppError::not_found(format!("User '{}' not found", username)))?;

    sqlx::query(
        "DELETE FROM follows WHERE follower_id = $1 AND following_id = $2"
    )
    .bind(auth.user_id)
    .bind(target)
    .execute(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Failed to unfollow: {}", e);
        AppError::internal("Failed to unfollow user")
    })?;

    Ok(Json(FollowResponse {
        message: format!("Unfollowed {}", username),
    }))
}

/// GET /users/:username/followers — list who follows this user
pub async fn get_followers(
    State(state): State<AppState>,
    Path(username): Path<String>,
) -> Result<Json<Vec<UserResponse>>, AppError> {

    let users = sqlx::query_as::<_, crate::models::UserRow>(
        r#"
        SELECT u.*
        FROM users u
        JOIN follows f ON u.id = f.follower_id
        JOIN users target ON f.following_id = target.id
        WHERE target.username = $1
        ORDER BY f.created_at DESC
        "#
    )
    .bind(&username)
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Database error: {}", e);
        AppError::internal("Database error")
    })?;

    let responses: Vec<UserResponse> = users.into_iter().map(UserResponse::from).collect();
    Ok(Json(responses))
}

/// GET /users/:username/following — list who this user follows
pub async fn get_following(
    State(state): State<AppState>,
    Path(username): Path<String>,
) -> Result<Json<Vec<UserResponse>>, AppError> {

    let users = sqlx::query_as::<_, crate::models::UserRow>(
        r#"
        SELECT u.*
        FROM users u
        JOIN follows f ON u.id = f.following_id
        JOIN users source ON f.follower_id = source.id
        WHERE source.username = $1
        ORDER BY f.created_at DESC
        "#
    )
    .bind(&username)
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Database error: {}", e);
        AppError::internal("Database error")
    })?;

    let responses: Vec<UserResponse> = users.into_iter().map(UserResponse::from).collect();
    Ok(Json(responses))
}

/// GET /users/:username/stats — public profile stats
pub async fn get_user_stats(
    State(state): State<AppState>,
    Path(username): Path<String>,
) -> Result<Json<ProfileStats>, AppError> {

    // Get user ID first
    let user_id = sqlx::query_scalar::<_, uuid::Uuid>(
        "SELECT id FROM users WHERE username = $1"
    )
    .bind(&username)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Database error: {}", e);
        AppError::internal("Database error")
    })?
    .ok_or_else(|| AppError::not_found(format!("User '{}' not found", username)))?;

    // Count followers, following, posts in parallel
    let (followers, following, posts) = tokio::try_join!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM follows WHERE following_id = $1")
            .bind(user_id)
            .fetch_one(&state.db),
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM follows WHERE follower_id = $1")
            .bind(user_id)
            .fetch_one(&state.db),
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM posts WHERE user_id = $1")
            .bind(user_id)
            .fetch_one(&state.db),
    )
    .map_err(|e| {
        tracing::error!("Database error: {}", e);
        AppError::internal("Database error")
    })?;

    Ok(Json(ProfileStats {
        followers,
        following,
        posts,
    }))
}
