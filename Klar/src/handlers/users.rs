use axum::{
    extract::{Multipart, Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use uuid::Uuid;
use argon2::{Argon2, password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString}};

use crate::auth::AuthUser;
use crate::errors::AppError;
use crate::handlers::auth::AppState;
use crate::media;
use crate::models::{UpdateProfileRequest, UserResponse, UserRow, UserPublicResponse};
use chrono::{Duration, Utc};

/// Search query parameters
#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub q: String,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// GET /users/search?q=term — search users by username or display name
pub async fn search_users(
    State(state): State<AppState>,
    Query(params): Query<SearchQuery>,
) -> Result<Json<Vec<UserPublicResponse>>, AppError> {
    let query = params.q.trim().to_string();

    if query.is_empty() {
        return Err(AppError::bad_request("Search query cannot be empty"));
    }
    if query.len() > 100 {
        return Err(AppError::bad_request("Search query too long"));
    }

    let limit = params.limit.unwrap_or(20).min(50).max(1);
    let offset = params.offset.unwrap_or(0).max(0);
    let pattern = format!("%{}%", query);

    let users = sqlx::query_as::<_, UserRow>(
        r#"
        SELECT * FROM users
        WHERE username ILIKE $1 OR display_name ILIKE $1
        ORDER BY
            CASE WHEN username ILIKE $2 THEN 0 ELSE 1 END,
            username
        LIMIT $3 OFFSET $4
        "#
    )
    .bind(&pattern)
    .bind(&format!("{}%", query))
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Search query failed: {}", e);
        AppError::internal("Search failed")
    })?;

    Ok(Json(users.into_iter().map(UserPublicResponse::from).collect()))
}

/// GET /users/:username — public profile
pub async fn get_user(
    State(state): State<AppState>,
    Path(username): Path<String>,
) -> Result<Json<UserPublicResponse>, AppError> {
    let user = sqlx::query_as::<_, UserRow>(
        "SELECT * FROM users WHERE username = $1"
    )
    .bind(&username)
    .fetch_optional(&state.db)
    .await;

    match user {
        Ok(Some(user)) => Ok(Json(UserPublicResponse::from(user))),
        Ok(None) => Err(AppError::not_found(format!("User '{}' not found", username))),
        Err(e) => {
            tracing::error!("Database error: {}", e);
            Err(AppError::internal("Database error"))
        }
    }
}

/// GET /users/me — own profile (auth required)
pub async fn get_me(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<UserPublicResponse>, AppError> {
    let user = sqlx::query_as::<_, UserRow>(
        "SELECT * FROM users WHERE id = $1"
    )
    .bind(auth.user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Database error: {}", e);
        AppError::internal("Database error")
    })?;

    match user {
        Some(user) => Ok(Json(UserPublicResponse::from(user))),
        None => Err(AppError::not_found("User not found")),
    }
}

/// PATCH /users/me
pub async fn update_profile(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<UpdateProfileRequest>,
) -> Result<Json<UserResponse>, AppError> {

    // 1. Fetch current user to check the cooldown
    let current_user = sqlx::query_as::<_, UserRow>("SELECT * FROM users WHERE id = $1")
        .bind(auth.user_id)
        .fetch_one(&state.db)
        .await
        .map_err(|_| AppError::internal("Database error"))?;

    let mut final_username = input.username.clone();

    // 2. Handle Username Logic (Validation)
    if let Some(new_username) = &input.username {
        let formatted_username = new_username.trim().to_lowercase();
        final_username = Some(formatted_username.clone());
        
        if formatted_username != current_user.username {
            // Check length/format
            if formatted_username.len() < 3 || formatted_username.len() > 30 {
                return Err(AppError::bad_request("Username must be between 3 and 30 characters"));
            }

            // Check 14-day cooldown
            if let Some(last_changed) = current_user.username_changed_at {
                if Utc::now() - last_changed < Duration::days(14) {
                    let available_at = last_changed + Duration::days(14);
                    return Err(AppError::bad_request(format!(
                        "You can change your username again on {}", 
                        available_at.format("%Y-%m-%d")
                    )));
                }
            }

            // Check if username is taken
            let taken = sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM users WHERE username = $1)")
                .bind(&formatted_username)
                .fetch_one(&state.db)
                .await
                .unwrap_or(true);

            if taken {
                return Err(AppError::conflict("Username is already taken"));
            }
        }
    }

    // 3. Execute the unified COALESCE query
    let updated_user = sqlx::query_as::<_, UserRow>(
        r#"
        UPDATE users 
        SET 
            username = COALESCE($1, username),
            username_changed_at = CASE WHEN $1 IS NOT NULL AND $1 != username THEN NOW() ELSE username_changed_at END,
            display_name = COALESCE($2, display_name),
            bio = COALESCE($3, bio)
        WHERE id = $4
        RETURNING *
        "#
    )
    .bind(&final_username)
    .bind(&input.display_name)
    .bind(&input.bio)
    .bind(auth.user_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        let msg = e.to_string();
        if msg.contains("duplicate key") {
            AppError::conflict("Username is already taken")
        } else {
            tracing::error!("Update failed: {}", msg);
            AppError::internal("Failed to update profile")
        }
    })?;

    Ok(Json(UserResponse::from(updated_user)))
}

/// POST /users/me/avatar — upload avatar image (auth required)
pub async fn upload_avatar(
    State(state): State<AppState>,
    auth: AuthUser,
    mut multipart: Multipart,
) -> Result<Json<UserResponse>, AppError> {
    let mut image_data: Option<Vec<u8>> = None;

    while let Some(field) = multipart.next_field().await
        .map_err(|e| AppError::bad_request(format!("Invalid multipart data: {}", e)))?
    {
        let name = field.name().unwrap_or("").to_string();
        if name == "avatar" {
            let bytes = field.bytes().await
                .map_err(|e| AppError::bad_request(format!("Failed to read image: {}", e)))?;
            if bytes.len() > 5 * 1024 * 1024 {
                return Err(AppError::bad_request("Avatar must be under 5MB"));
            }
            if bytes.is_empty() {
                return Err(AppError::bad_request("Avatar file is empty"));
            }
            image_data = Some(bytes.to_vec());
        }
    }

    let raw_bytes = image_data.ok_or_else(|| AppError::bad_request("Avatar field is required"))?;

    let processed = tokio::task::spawn_blocking(move || media::process_image(&raw_bytes))
        .await
        .map_err(|e| AppError::internal(format!("Processing task failed: {:?}", e)))?
        .map_err(|e| AppError::bad_request(format!("Image processing failed: {:?}", e)))?;

    let avatar_id = Uuid::new_v4();
    let avatar_key = format!("avatars/{}.jpg", avatar_id);

    state.storage.save(&avatar_key, &processed.thumb).await
        .map_err(|e| AppError::internal(format!("Failed to save avatar: {:?}", e)))?;

    // Delete old avatar file if one exists
    let old_avatar = sqlx::query_scalar::<_, Option<String>>(
        "SELECT avatar_url FROM users WHERE id = $1"
    )
    .bind(auth.user_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Database error: {}", e);
        AppError::internal("Database error")
    })?;

    if let Some(old_url) = old_avatar {
        let old_key = old_url.strip_prefix("/media/").unwrap_or(&old_url);
        let _ = state.storage.delete(old_key).await;
    }

    let user = sqlx::query_as::<_, UserRow>(
        "UPDATE users SET avatar_url = $1 WHERE id = $2 RETURNING *"
    )
    .bind(&avatar_key)
    .bind(auth.user_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Failed to update avatar: {}", e);
        AppError::internal("Failed to update avatar")
    })?;

    tracing::info!("Avatar updated: {}", auth.user_id);
    Ok(Json(UserResponse::from(user)))
}


#[derive(Debug, Deserialize)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

/// PATCH /users/me/password — change password (auth required)
/// Requires current password to verify identity before updating.
/// Invalidates all refresh tokens on success to force re-login on other devices.
pub async fn change_password(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<ChangePasswordRequest>,
) -> Result<StatusCode, AppError> {

    if input.new_password.len() < 8 {
        return Err(AppError::bad_request("Password must be at least 8 characters"));
    }
    if input.current_password == input.new_password {
        return Err(AppError::bad_request("New password must be different from current password"));
    }

    // Fetch current password hash
    let stored_hash = sqlx::query_scalar::<_, Option<String>>(
        "SELECT password_hash FROM users WHERE id = $1"
    )
    .bind(auth.user_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| { tracing::error!("Database error: {}", e); AppError::internal("Database error") })?
    .ok_or_else(|| AppError::bad_request("Invalid current password"))?;

    // Verify current password
    let parsed_hash = PasswordHash::new(&stored_hash)
        .map_err(|_| AppError::internal("Failed to parse password hash"))?;
    Argon2::default()
        .verify_password(input.current_password.as_bytes(), &parsed_hash)
        .map_err(|_| AppError::bad_request("Current password is incorrect"))?;

    // Hash new password
    let salt = SaltString::generate(&mut OsRng);
    let new_hash = Argon2::default()
        .hash_password(input.new_password.as_bytes(), &salt)
        .map_err(|_| AppError::internal("Failed to hash password"))?
        .to_string();

    // Update password
    sqlx::query("UPDATE users SET password_hash = $1 WHERE id = $2")
        .bind(&new_hash)
        .bind(auth.user_id)
        .execute(&state.db)
        .await
        .map_err(|e| { tracing::error!("Failed to update password: {}", e); AppError::internal("Failed to update password") })?;

    // Invalidate all refresh tokens — force re-login on other devices
    sqlx::query("DELETE FROM refresh_tokens WHERE user_id = $1")
        .bind(auth.user_id)
        .execute(&state.db)
        .await
        .map_err(|e| { tracing::error!("Failed to invalidate sessions: {}", e); AppError::internal("Database error") })?;

    tracing::info!("Password changed for user: {}", auth.user_id);
    Ok(StatusCode::NO_CONTENT)
}

/// DELETE /users/me — delete account and all associated data (auth required)
///
/// Deletion order:
/// 1. Fetch all media file keys for this user's posts (before CASCADE removes them)
/// 2. Fetch avatar key
/// 3. Delete the user record (CASCADE handles all DB relations)
/// 4. Delete media files from disk
pub async fn delete_account(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<StatusCode, AppError> {

    // Collect all media file keys for this user's posts
    let media_keys = sqlx::query_as::<_, (String, String, String)>(
        r#"
        SELECT ma.thumb_key, ma.medium_key, ma.full_key
        FROM media_assets ma
        JOIN posts p ON ma.post_id = p.id
        WHERE p.user_id = $1
        "#
    )
    .bind(auth.user_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Database error: {}", e);
        AppError::internal("Database error")
    })?;

    // Get avatar key
    let avatar_url = sqlx::query_scalar::<_, Option<String>>(
        "SELECT avatar_url FROM users WHERE id = $1"
    )
    .bind(auth.user_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Database error: {}", e);
        AppError::internal("Database error")
    })?;

    // Delete user — CASCADE removes posts, comments, likes, follows, blocks,
    // refresh_tokens, email_tokens, media_asset rows
    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(auth.user_id)
        .execute(&state.db)
        .await
        .map_err(|e| {
            tracing::error!("Failed to delete user: {}", e);
            AppError::internal("Failed to delete account")
        })?;

    // Clean up files from disk (best-effort — orphaned files are acceptable)
    for (thumb, medium, full) in &media_keys {
        let _ = state.storage.delete(thumb).await;
        let _ = state.storage.delete(medium).await;
        let _ = state.storage.delete(full).await;
    }

    if let Some(url) = avatar_url {
        let key = url.strip_prefix("/media/").unwrap_or(&url);
        let _ = state.storage.delete(key).await;
    }

    tracing::info!("Account deleted: {}", auth.user_id);
    Ok(StatusCode::NO_CONTENT)
}