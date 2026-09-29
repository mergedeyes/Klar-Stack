use axum::{
    extract::{Multipart, Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use uuid::Uuid;
use argon2::{Argon2, password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString}};

use crate::auth::{AuthUser, OptionalAuthUser};
use crate::errors::AppError;
use crate::handlers::auth::AppState;
use crate::handlers::follows::{has_pending_follow_request, is_following};
use crate::evidence;
use crate::media;
use crate::models::{UpdateProfileRequest, UserResponse, UserRow, UserPublicResponse};
use crate::utils::{delete_media, DbResultExt, ResolveMedia};
use crate::validation::{
    check_max_len, escape_like, page_limit, validate_password, validate_username, BIO_MAX, DISPLAY_NAME_MAX,
};
use chrono::{DateTime, Duration, Utc};
use std::io::{Seek, Write};

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

    let limit = page_limit(params.limit, 20, 50);
    let offset = params.offset.unwrap_or(0).max(0);
    // Escaped so "%" or "_" in the query match literally instead of
    // acting as wildcards.
    let escaped = escape_like(&query);
    let pattern = format!("%{}%", escaped);

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
    .bind(format!("{}%", escaped))
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.db)
    .await
    .db_err_ctx("Search query failed", "Search failed")?;

    // No viewer_relationship computed here (would be N extra lookups per
    // result) -- search results only need is_private, to show a lock icon;
    // the profile page itself computes the real relationship when opened.
    let responses: Vec<UserPublicResponse> = users.into_iter().map(UserPublicResponse::from).collect();
    Ok(Json(responses.resolve_media(&state.storage)))
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
    .db_err("Database error")?
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

    Ok(Json(response.resolve_media(&state.storage)))
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
    .db_err("Database error")?;

    match user {
        Some(user) => {
            // Computed from `user.email`/`user.email_verified` before
            // `user` moves into UserPublicResponse::from(user) below --
            // that conversion doesn't carry either through (see its own
            // doc comment), so it has to happen here, while `user` is
            // still owned. email_verified is required, not just an email
            // match -- see reports.rs's require_admin doc comment for why
            // (this is the same rule, applied here so the displayed menu
            // item and the real server-side check never disagree).
            let is_admin = user.email_verified && crate::utils::is_admin_email(&user.email);
            let (email, email_verified) = (user.email.clone(), user.email_verified);
            let mut response = UserPublicResponse::from(user);
            response.viewer_relationship = Some("self".to_string());
            response.is_admin = is_admin;
            response.email = Some(email);
            response.email_verified = Some(email_verified);
            Ok(Json(response.resolve_media(&state.storage)))
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
        .db_err("Database error")?;

    if let Some(display_name) = &input.display_name {
        check_max_len(display_name, "Display name", DISPLAY_NAME_MAX)?;
    }
    if let Some(bio) = &input.bio {
        check_max_len(bio, "Bio", BIO_MAX)?;
    }

    let mut final_username = input.username.clone();

    // 2. Handle Username Logic (Validation)
    if let Some(new_username) = &input.username {
        // Case is preserved exactly as entered for storage; only the
        // comparison below (and the "taken" check) is case-insensitive.
        let formatted_username = new_username.trim().to_string();
        final_username = Some(formatted_username.clone());

        if formatted_username.to_lowercase() != current_user.username.to_lowercase() {
            // Only a *changed* name is validated -- re-casing your own name
            // skips this, so accounts created before these rules existed
            // aren't forced to rename.
            validate_username(&formatted_username)?;

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
    .bind(input.is_private)
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

    Ok(Json(UserResponse::from(updated_user).resolve_media(&state.storage)))
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
    // process_image always re-encodes to WebP (same as post uploads); the
    // extension also sets the Content-Type storage.save() uploads with.
    let avatar_key = format!("avatars/{}.webp", avatar_id);

    state.storage.save(&avatar_key, &processed.thumb).await
        .map_err(|e| AppError::internal(format!("Failed to save avatar: {:?}", e)))?;

    // Delete old avatar file if one exists
    let old_avatar = sqlx::query_scalar::<_, Option<String>>(
        "SELECT avatar_url FROM users WHERE id = $1"
    )
    .bind(auth.user_id)
    .fetch_one(&state.db)
    .await
    .db_err("Database error")?;

    if let Some(old_url) = old_avatar {
        let old_key = old_url.strip_prefix("/media/").unwrap_or(&old_url);
        delete_media(&state, old_key).await;
    }

    let user = sqlx::query_as::<_, UserRow>(
        "UPDATE users SET avatar_url = $1 WHERE id = $2 RETURNING *"
    )
    .bind(&avatar_key)
    .bind(auth.user_id)
    .fetch_one(&state.db)
    .await
    .db_err("Failed to update avatar")?;

    tracing::info!("Avatar updated: {}", auth.user_id);
    Ok(Json(UserResponse::from(user).resolve_media(&state.storage)))
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

    validate_password(&input.new_password)?;
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
    .db_err("Database error")?
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
        .db_err("Failed to update password")?;

    // Invalidate all refresh tokens — force re-login on other devices
    sqlx::query("DELETE FROM refresh_tokens WHERE user_id = $1")
        .bind(auth.user_id)
        .execute(&state.db)
        .await
        .db_err_ctx("Failed to invalidate sessions", "Database error")?;

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
    .db_err("Database error")?;

    // Get avatar key
    let avatar_url = sqlx::query_scalar::<_, Option<String>>(
        "SELECT avatar_url FROM users WHERE id = $1"
    )
    .bind(auth.user_id)
    .fetch_one(&state.db)
    .await
    .db_err("Database error")?;

    let mut tx = state.db.begin().await.db_err_ctx("Failed to start transaction", "Database error")?;

    // The deletion goes ahead (Art. 17), but content with a pending
    // likely-illegal report -- this user's posts and comments, comments
    // under their posts, and the profile itself -- is preserved as
    // evidence first (Art. 17(3)(e)); see evidence.rs.
    let scope = evidence::Scope {
        posts: sqlx::query_scalar::<_, Uuid>("SELECT id FROM posts WHERE user_id = $1")
            .bind(auth.user_id)
            .fetch_all(&mut *tx)
            .await
            .db_err_ctx("Failed to list posts", "Failed to delete account")?,
        comments: sqlx::query_scalar::<_, Uuid>("SELECT id FROM comments WHERE user_id = $1")
            .bind(auth.user_id)
            .fetch_all(&mut *tx)
            .await
            .db_err_ctx("Failed to list comments", "Failed to delete account")?,
        user: Some(auth.user_id),
    };
    let preserved = evidence::preserve(&mut tx, &scope, evidence::Trigger::AccountDeletion, Some(auth.user_id)).await?;

    // Conversations whose other participant already deleted their account
    // would be left with nobody in them once this user goes too -- remove
    // them outright. Conversations with a remaining participant are kept
    // for that person; this user's side becomes NULL ("Deleted User") via
    // ON DELETE SET NULL (migration 20260929000100), and this user's
    // messages in it are erased via ON DELETE CASCADE (20260930000000).
    sqlx::query(
        "DELETE FROM conversations WHERE (user1_id = $1 AND user2_id IS NULL) OR (user2_id = $1 AND user1_id IS NULL)"
    )
    .bind(auth.user_id)
    .execute(&mut *tx)
    .await
    .db_err_ctx("Failed to delete orphaned conversations", "Failed to delete account")?;

    // Delete user — CASCADE removes posts, comments, likes, follows, blocks,
    // sent messages, refresh_tokens, email_tokens, media_asset rows
    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(auth.user_id)
        .execute(&mut *tx)
        .await
        .db_err("Failed to delete account")?;

    tx.commit().await.db_err_ctx("Failed to commit transaction", "Failed to delete account")?;

    // Clean up the files and their CDN copies (best-effort, failures are
    // logged by delete_media), apart from preserved ones still waiting
    // for their evidence copy.
    let avatar_key = avatar_url.map(|url| url.strip_prefix("/media/").unwrap_or(&url).to_string());
    let keys = media_keys
        .into_iter()
        .flat_map(|(thumb, medium, full)| [thumb, medium, full])
        .chain(avatar_key);
    evidence::finish(&state, preserved, keys).await;

    tracing::info!("Account deleted: {}", auth.user_id);
    Ok(StatusCode::NO_CONTENT)
}

/// GET /users/me/export — self-service data export (Art. 15 + Art. 20 DSGVO:
/// right of access + right to data portability). Returns everything we hold
/// about the requesting user as a ZIP: a pretty-printed data.json plus the
/// uploaded images themselves — no admin/manual DB query needed on our end.
///
/// The images are included as files, not links: media URLs are signed and
/// expire within hours (storage.rs), so a link in an export would stop
/// working long before the person is done with their copy.
///
/// Deliberately excludes: password_hash, refresh/email tokens (security
/// artifacts, not meaningful personal data the person would want back).
pub async fn export_my_data(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<axum::response::Response, AppError> {
    // --- Profile ---
    // terms_accepted_at is included here (not just internally on UserRow)
    // because this export is meant to be "everything we hold about you" --
    // proof of when ToS/privacy consent was given is squarely personal
    // data about the account, even though it's never shown in the app UI.
    let profile = sqlx::query_as::<_, (String, String, Option<String>, Option<String>, Option<String>, bool, DateTime<Utc>, Option<DateTime<Utc>>)>(
        "SELECT username, email, display_name, bio, avatar_url, email_verified, created_at, terms_accepted_at FROM users WHERE id = $1"
    )
    .bind(auth.user_id)
    .fetch_one(&state.db)
    .await
    .db_err_ctx("Data export query failed", "Database error")?;

    // --- Posts (with their media) ---
    let posts = sqlx::query_as::<_, (Uuid, Option<String>, i64, i64, DateTime<Utc>, Option<DateTime<Utc>>)>(
        "SELECT id, caption, like_count, comment_count, created_at, edited_at FROM posts WHERE user_id = $1 ORDER BY created_at DESC"
    )
    .bind(auth.user_id)
    .fetch_all(&state.db)
    .await
    .db_err_ctx("Data export query failed", "Database error")?;

    let post_ids: Vec<Uuid> = posts.iter().map(|p| p.0).collect();

    let media_rows = if post_ids.is_empty() {
        vec![]
    } else {
        sqlx::query_as::<_, (Uuid, String, i32, i32, i32)>(
            "SELECT post_id, full_key, width, height, sort_order FROM media_assets WHERE post_id = ANY($1) ORDER BY sort_order"
        )
        .bind(&post_ids)
        .fetch_all(&state.db)
        .await
        .db_err_ctx("Data export query failed", "Database error")?
    };

    // (path inside the ZIP, storage key) of every file to bundle. Only the
    // full-size variant is exported; thumb/medium are derived from it.
    let mut export_files: Vec<(String, String)> = Vec::new();

    let posts_json: Vec<serde_json::Value> = posts.into_iter().map(|(id, caption, like_count, comment_count, created_at, edited_at)| {
        let media: Vec<serde_json::Value> = media_rows.iter()
            .filter(|m| m.0 == id)
            .map(|(_, full, width, height, sort_order)| {
                let path = format!("media/{}/{}.{}", id, sort_order, file_extension(full));
                export_files.push((path.clone(), full.clone()));
                serde_json::json!({
                    "file": path,
                    "width": width,
                    "height": height,
                })
            })
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
    .db_err_ctx("Data export query failed", "Database error")?;

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
    .db_err_ctx("Data export query failed", "Database error")?;

    // --- Likes given (comments) ---
    let comment_likes = sqlx::query_as::<_, (Uuid, DateTime<Utc>)>(
        "SELECT comment_id, created_at FROM comment_likes WHERE user_id = $1 ORDER BY created_at DESC"
    )
    .bind(auth.user_id)
    .fetch_all(&state.db)
    .await
    .db_err_ctx("Data export query failed", "Database error")?;

    // --- Following / Followers ---
    let following = sqlx::query_as::<_, (String, DateTime<Utc>)>(
        "SELECT u.username, f.created_at FROM follows f JOIN users u ON u.id = f.following_id WHERE f.follower_id = $1 ORDER BY f.created_at DESC"
    )
    .bind(auth.user_id)
    .fetch_all(&state.db)
    .await
    .db_err_ctx("Data export query failed", "Database error")?;

    let followers = sqlx::query_as::<_, (String, DateTime<Utc>)>(
        "SELECT u.username, f.created_at FROM follows f JOIN users u ON u.id = f.follower_id WHERE f.following_id = $1 ORDER BY f.created_at DESC"
    )
    .bind(auth.user_id)
    .fetch_all(&state.db)
    .await
    .db_err_ctx("Data export query failed", "Database error")?;

    // --- Blocked users (that this account initiated) ---
    let blocked = sqlx::query_as::<_, (String, DateTime<Utc>)>(
        "SELECT u.username, b.created_at FROM blocks b JOIN users u ON u.id = b.blocked_id WHERE b.blocker_id = $1 ORDER BY b.created_at DESC"
    )
    .bind(auth.user_id)
    .fetch_all(&state.db)
    .await
    .db_err_ctx("Data export query failed", "Database error")?;

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
    .db_err_ctx("Data export query failed", "Database error")?;

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
    // LEFT JOINs: the other participant may have deleted their account
    // (NULL user id), and the conversation still belongs in this export.
    let conversations = sqlx::query_as::<_, (Uuid, String)>(
        r#"
        SELECT c.id,
            COALESCE(CASE WHEN c.user1_id = $1 THEN u2.username ELSE u1.username END, 'Deleted User')
        FROM conversations c
        LEFT JOIN users u1 ON u1.id = c.user1_id
        LEFT JOIN users u2 ON u2.id = c.user2_id
        WHERE c.user1_id = $1 OR c.user2_id = $1
        ORDER BY c.updated_at DESC
        "#
    )
    .bind(auth.user_id)
    .fetch_all(&state.db)
    .await
    .db_err_ctx("Data export query failed", "Database error")?;

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
        .db_err_ctx("Data export query failed", "Database error")?;

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

    let avatar_file = profile.4.as_deref().map(|url| {
        let key = url.strip_prefix("/media/").unwrap_or(url);
        let path = format!("avatar.{}", file_extension(key));
        export_files.push((path.clone(), key.to_string()));
        path
    });

    // Written to an anonymous temp file rather than memory, so an account
    // with thousands of images can't exhaust the container's RAM; the OS
    // removes the file once the response has been streamed and it's closed.
    let tmp_err = |e: std::io::Error| {
        tracing::error!("Data export temp file error: {}", e);
        AppError::internal("Failed to build export")
    };
    let zip_err = |e: zip::result::ZipError| {
        tracing::error!("Data export zip error: {}", e);
        AppError::internal("Failed to build export")
    };
    let mut zip = zip::ZipWriter::new(tempfile::tempfile().map_err(tmp_err)?);
    // Images are already compressed (WebP/JPEG), deflating them again only
    // costs CPU.
    let stored = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Stored);
    let deflated = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    // A file that can't be fetched shouldn't sink the whole export (the
    // person is entitled to the rest); it's listed in data.json instead,
    // which is why data.json is written last.
    let mut missing_files = Vec::new();
    for (path, key) in &export_files {
        match state.storage.get(key).await {
            Ok(bytes) => {
                zip.start_file(path.as_str(), stored).map_err(zip_err)?;
                zip.write_all(&bytes).map_err(tmp_err)?;
            }
            Err(_) => {
                tracing::error!("Data export for {}: could not fetch {}", auth.user_id, key);
                missing_files.push(path.clone());
            }
        }
    }

    let export = serde_json::json!({
        "export_info": {
            "generated_at": Utc::now(),
            "note": "Datenexport gemäß Art. 15/20 DSGVO — alle personenbezogenen Daten, die Klar über diesen Account gespeichert hat. Bilder liegen als Dateien in diesem Archiv; die \"file\"-Felder geben ihren Pfad an.",
            "missing_files": missing_files,
        },
        "profile": {
            "username": profile.0,
            "email": profile.1,
            "display_name": profile.2,
            "bio": profile.3,
            "avatar_file": avatar_file,
            "email_verified": profile.5,
            "created_at": profile.6,
            "terms_accepted_at": profile.7,
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

    // Pretty-printed, not the compact single-line output axum's Json
    // wrapper would produce -- this file is meant to be opened and read
    // by the person it belongs to, not just machine-parsed. (GDPR Art. 15
    // "commonly used, machine-readable format" doesn't preclude also
    // being human-readable.) Building the Response by hand (rather than
    // returning (HeaderMap, Json<..>)) is what makes that possible --
    // Json's own IntoResponse impl always serializes compact, with no
    // pretty-print option.
    let pretty = serde_json::to_string_pretty(&export)
        .unwrap_or_else(|_| export.to_string());

    zip.start_file("data.json", deflated).map_err(zip_err)?;
    zip.write_all(pretty.as_bytes()).map_err(tmp_err)?;

    let mut file = zip.finish().map_err(zip_err)?;
    let size = file.seek(std::io::SeekFrom::End(0)).map_err(tmp_err)?;
    file.seek(std::io::SeekFrom::Start(0)).map_err(tmp_err)?;
    let body = axum::body::Body::from_stream(tokio_util::io::ReaderStream::new(
        tokio::fs::File::from_std(file),
    ));

    let filename = format!("klar-datenexport-{}.zip", Utc::now().format("%Y-%m-%d"));

    tracing::info!("Data export generated for user: {} ({} bytes)", auth.user_id, size);

    let response = axum::response::Response::builder()
        .status(StatusCode::OK)
        .header(axum::http::header::CONTENT_TYPE, "application/zip")
        .header(axum::http::header::CONTENT_LENGTH, size)
        .header(
            axum::http::header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}\"", filename),
        )
        .body(body)
        .map_err(|_| AppError::internal("Failed to build export response"))?;

    Ok(response)
}

/// Extension of a storage key ("full/<id>.webp" -> "webp"), so exported
/// files open with the right program. Keys are always generated with one.
fn file_extension(key: &str) -> &str {
    key.rsplit_once('.').map(|(_, ext)| ext).unwrap_or("bin")
}
