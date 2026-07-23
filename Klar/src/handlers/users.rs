use axum::{
    extract::{Multipart, Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    Json,
};
use serde::Deserialize;
use uuid::Uuid;
use argon2::{Argon2, password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString}};

use crate::auth::{AuthUser, OptionalAuthUser};
use crate::errors::AppError;
use crate::handlers::auth::AppState;
use crate::handlers::follows::{has_pending_follow_request, is_following};
use crate::media;
use crate::models::{UpdateProfileRequest, UserResponse, UserRow, UserPublicResponse};
use chrono::{DateTime, Duration, Utc};

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

    // No viewer_relationship computed here (would be N extra lookups per
    // result) -- search results only need is_private, to show a lock icon;
    // the profile page itself computes the real relationship when opened.
    Ok(Json(users.into_iter().map(UserPublicResponse::from).collect()))
}

/// GET /users/:username — public profile. Uses OptionalAuthUser (not
/// AuthUser) since profiles are viewable while logged out -- viewer_relationship
/// is just None in that case.
pub async fn get_user(
    State(state): State<AppState>,
    auth: OptionalAuthUser,
    Path(username): Path<String>,
) -> Result<Json<UserPublicResponse>, AppError> {
    let user = sqlx::query_as::<_, UserRow>(
        "SELECT * FROM users WHERE LOWER(username) = LOWER($1)"
    )
    .bind(&username)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Database error: {}", e);
        AppError::internal("Database error")
    })?
    .ok_or_else(|| AppError::not_found(format!("User '{}' not found", username)))?;

    let profile_id = user.id;
    let mut response = UserPublicResponse::from(user);

    match auth.user_id {
        None => {}
        Some(viewer_id) if viewer_id == profile_id => {
            response.viewer_relationship = Some("self".to_string());
        }
        Some(viewer_id) => {
            response.viewer_relationship = if is_following(&state.db, viewer_id, profile_id).await? {
                Some("following".to_string())
            } else if has_pending_follow_request(&state.db, viewer_id, profile_id).await? {
                Some("requested".to_string())
            } else {
                Some("not_following".to_string())
            };

            // Reverse direction: does *this profile* have a pending
            // request to follow *me* (the viewer)? Lets accept/decline
            // show up directly on the requester's own profile page, not
            // only in the notification dropdown.
            response.incoming_follow_request = has_pending_follow_request(&state.db, profile_id, viewer_id).await?;
        }
    };

    Ok(Json(response))
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
        Some(user) => {
            let mut response = UserPublicResponse::from(user);
            response.viewer_relationship = Some("self".to_string());
            Ok(Json(response))
        }
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
        // Case is preserved exactly as entered for storage; only the
        // comparison below (and the "taken" check) is case-insensitive.
        let formatted_username = new_username.trim().to_string();
        final_username = Some(formatted_username.clone());

        if formatted_username.to_lowercase() != current_user.username.to_lowercase() {
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

            // Check if username is taken (case-insensitively; excluding the
            // current user so re-casing your own name never conflicts with
            // yourself, e.g. "johndoe" -> "JohnDoe")
            let taken = sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM users WHERE LOWER(username) = LOWER($1) AND id != $2)"
            )
                .bind(&formatted_username)
                .bind(auth.user_id)
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
            username_changed_at = CASE WHEN $1 IS NOT NULL AND LOWER($1) != LOWER(username) THEN NOW() ELSE username_changed_at END,
            display_name = COALESCE($2, display_name),
            bio = COALESCE($3, bio),
            is_private = COALESCE($5, is_private)
        WHERE id = $4
        RETURNING *
        "#
    )
    .bind(&final_username)
    .bind(&input.display_name)
    .bind(&input.bio)
    .bind(auth.user_id)
    .bind(&input.is_private)
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

/// GET /users/me/export — self-service data export (Art. 15 + Art. 20 DSGVO:
/// right of access + right to data portability). Returns everything we hold
/// about the requesting user as a single, pretty-printed, downloadable JSON
/// file — no admin/manual DB query needed on our end.
///
/// Deliberately excludes: password_hash, refresh/email tokens (security
/// artifacts, not meaningful personal data the person would want back).
pub async fn export_my_data(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<(HeaderMap, Json<serde_json::Value>), AppError> {
    let db_err = |e: sqlx::Error| {
        tracing::error!("Data export query failed: {}", e);
        AppError::internal("Database error")
    };

    // --- Profile ---
    let profile = sqlx::query_as::<_, (String, String, Option<String>, Option<String>, Option<String>, bool, DateTime<Utc>)>(
        "SELECT username, email, display_name, bio, avatar_url, email_verified, created_at FROM users WHERE id = $1"
    )
    .bind(auth.user_id)
    .fetch_one(&state.db)
    .await
    .map_err(db_err)?;

    // --- Posts (with their media) ---
    let posts = sqlx::query_as::<_, (Uuid, Option<String>, i64, i64, DateTime<Utc>, Option<DateTime<Utc>>)>(
        "SELECT id, caption, like_count, comment_count, created_at, edited_at FROM posts WHERE user_id = $1 ORDER BY created_at DESC"
    )
    .bind(auth.user_id)
    .fetch_all(&state.db)
    .await
    .map_err(db_err)?;

    let post_ids: Vec<Uuid> = posts.iter().map(|p| p.0).collect();

    let media_rows = if post_ids.is_empty() {
        vec![]
    } else {
        sqlx::query_as::<_, (Uuid, String, String, String, i32, i32)>(
            "SELECT post_id, thumb_key, medium_key, full_key, width, height FROM media_assets WHERE post_id = ANY($1)"
        )
        .bind(&post_ids)
        .fetch_all(&state.db)
        .await
        .map_err(db_err)?
    };

    let posts_json: Vec<serde_json::Value> = posts.into_iter().map(|(id, caption, like_count, comment_count, created_at, edited_at)| {
        let media: Vec<serde_json::Value> = media_rows.iter()
            .filter(|m| m.0 == id)
            .map(|(_, thumb, medium, full, width, height)| serde_json::json!({
                "thumbnail_url": state.storage.public_url(thumb),
                "medium_url": state.storage.public_url(medium),
                "full_url": state.storage.public_url(full),
                "width": width,
                "height": height,
            }))
            .collect();

        serde_json::json!({
            "id": id,
            "caption": caption,
            "like_count": like_count,
            "comment_count": comment_count,
            "created_at": created_at,
            "edited_at": edited_at,
            "media": media,
        })
    }).collect();

    // --- Comments ---
    let comments = sqlx::query_as::<_, (Uuid, Uuid, String, i64, DateTime<Utc>, Option<DateTime<Utc>>)>(
        "SELECT post_id, id, body, like_count, created_at, edited_at FROM comments WHERE user_id = $1 ORDER BY created_at DESC"
    )
    .bind(auth.user_id)
    .fetch_all(&state.db)
    .await
    .map_err(db_err)?;

    let comments_json: Vec<serde_json::Value> = comments.into_iter().map(|(post_id, id, body, like_count, created_at, edited_at)| {
        serde_json::json!({
            "id": id,
            "post_id": post_id,
            "body": body,
            "like_count": like_count,
            "created_at": created_at,
            "edited_at": edited_at,
        })
    }).collect();

    // --- Likes given (posts) ---
    let post_likes = sqlx::query_as::<_, (Uuid, DateTime<Utc>)>(
        "SELECT post_id, created_at FROM likes WHERE user_id = $1 ORDER BY created_at DESC"
    )
    .bind(auth.user_id)
    .fetch_all(&state.db)
    .await
    .map_err(db_err)?;

    // --- Likes given (comments) ---
    let comment_likes = sqlx::query_as::<_, (Uuid, DateTime<Utc>)>(
        "SELECT comment_id, created_at FROM comment_likes WHERE user_id = $1 ORDER BY created_at DESC"
    )
    .bind(auth.user_id)
    .fetch_all(&state.db)
    .await
    .map_err(db_err)?;

    // --- Following / Followers ---
    let following = sqlx::query_as::<_, (String, DateTime<Utc>)>(
        "SELECT u.username, f.created_at FROM follows f JOIN users u ON u.id = f.following_id WHERE f.follower_id = $1 ORDER BY f.created_at DESC"
    )
    .bind(auth.user_id)
    .fetch_all(&state.db)
    .await
    .map_err(db_err)?;

    let followers = sqlx::query_as::<_, (String, DateTime<Utc>)>(
        "SELECT u.username, f.created_at FROM follows f JOIN users u ON u.id = f.follower_id WHERE f.following_id = $1 ORDER BY f.created_at DESC"
    )
    .bind(auth.user_id)
    .fetch_all(&state.db)
    .await
    .map_err(db_err)?;

    // --- Blocked users (that this account initiated) ---
    let blocked = sqlx::query_as::<_, (String, DateTime<Utc>)>(
        "SELECT u.username, b.created_at FROM blocks b JOIN users u ON u.id = b.blocked_id WHERE b.blocker_id = $1 ORDER BY b.created_at DESC"
    )
    .bind(auth.user_id)
    .fetch_all(&state.db)
    .await
    .map_err(db_err)?;

    // --- Notifications received (capped — this is a personal export, not
    // an unbounded audit log) ---
    let notifications = sqlx::query_as::<_, (String, String, Option<Uuid>, Option<Uuid>, bool, DateTime<Utc>)>(
        r#"
        SELECT n.type::text, u.username, n.post_id, n.comment_id, n.is_read, n.created_at
        FROM notifications n JOIN users u ON u.id = n.actor_id
        WHERE n.user_id = $1
        ORDER BY n.created_at DESC
        LIMIT 500
        "#
    )
    .bind(auth.user_id)
    .fetch_all(&state.db)
    .await
    .map_err(db_err)?;

    let notifications_json: Vec<serde_json::Value> = notifications.into_iter().map(|(type_, actor_username, post_id, comment_id, is_read, created_at)| {
        serde_json::json!({
            "type": type_,
            "from": actor_username,
            "post_id": post_id,
            "comment_id": comment_id,
            "is_read": is_read,
            "created_at": created_at,
        })
    }).collect();

    // --- Conversations + messages ---
    // Includes the full shared conversation (both sides), matching how
    // WhatsApp/Instagram-style exports handle DMs — the alternative
    // (only your own sent messages) would produce a confusing, half-empty
    // conversation history for the person requesting their data.
    let conversations = sqlx::query_as::<_, (Uuid, String)>(
        r#"
        SELECT c.id,
            CASE WHEN c.user1_id = $1 THEN u2.username ELSE u1.username END
        FROM conversations c
        JOIN users u1 ON u1.id = c.user1_id
        JOIN users u2 ON u2.id = c.user2_id
        WHERE c.user1_id = $1 OR c.user2_id = $1
        ORDER BY c.updated_at DESC
        "#
    )
    .bind(auth.user_id)
    .fetch_all(&state.db)
    .await
    .map_err(db_err)?;

    let mut conversations_json = Vec::with_capacity(conversations.len());
    for (conv_id, other_username) in conversations {
        let messages = sqlx::query_as::<_, (String, String, DateTime<Utc>, Option<DateTime<Utc>>)>(
            r#"
            SELECT u.username, m.body, m.created_at, m.edited_at
            FROM messages m JOIN users u ON u.id = m.sender_id
            WHERE m.conversation_id = $1
            ORDER BY m.created_at
            "#
        )
        .bind(conv_id)
        .fetch_all(&state.db)
        .await
        .map_err(db_err)?;

        let messages_json: Vec<serde_json::Value> = messages.into_iter().map(|(sender, body, created_at, edited_at)| {
            serde_json::json!({
                "from": sender,
                "body": body,
                "created_at": created_at,
                "edited_at": edited_at,
            })
        }).collect();

        conversations_json.push(serde_json::json!({
            "with": other_username,
            "messages": messages_json,
        }));
    }

    let export = serde_json::json!({
        "export_info": {
            "generated_at": Utc::now(),
            "note": "Datenexport gemäß Art. 15/20 DSGVO — alle personenbezogenen Daten, die Klar über diesen Account gespeichert hat.",
        },
        "profile": {
            "username": profile.0,
            "email": profile.1,
            "display_name": profile.2,
            "bio": profile.3,
            "avatar_url": profile.4.map(|k| state.storage.public_url(&k)),
            "email_verified": profile.5,
            "created_at": profile.6,
        },
        "posts": posts_json,
        "comments": comments_json,
        "likes_given": {
            "posts": post_likes.into_iter().map(|(post_id, created_at)| serde_json::json!({"post_id": post_id, "created_at": created_at})).collect::<Vec<_>>(),
            "comments": comment_likes.into_iter().map(|(comment_id, created_at)| serde_json::json!({"comment_id": comment_id, "created_at": created_at})).collect::<Vec<_>>(),
        },
        "following": following.into_iter().map(|(username, since)| serde_json::json!({"username": username, "since": since})).collect::<Vec<_>>(),
        "followers": followers.into_iter().map(|(username, since)| serde_json::json!({"username": username, "since": since})).collect::<Vec<_>>(),
        "blocked_users": blocked.into_iter().map(|(username, since)| serde_json::json!({"username": username, "since": since})).collect::<Vec<_>>(),
        "notifications_received": notifications_json,
        "conversations": conversations_json,
    });

    let mut headers = HeaderMap::new();
    headers.insert(
        axum::http::header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!(
            "attachment; filename=\"klar-datenexport-{}.json\"",
            Utc::now().format("%Y-%m-%d")
        )).unwrap(),
    );

    tracing::info!("Data export generated for user: {}", auth.user_id);

    Ok((headers, Json(export)))
}
