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
use crate::moderation;
use crate::models::{RefreshResponse, UpdateProfileRequest, UserResponse, UserRow, UserPublicResponse};
use crate::standing;
use crate::utils::{DbResultExt, ResolveMedia};
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
        WHERE (username ILIKE $1 OR display_name ILIKE $1)
          AND (suspended_until IS NULL OR suspended_until <= NOW())
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
    // A suspended account's profile is hidden from everyone but itself
    // (standing.rs); to others it looks like it doesn't exist.
    let user = sqlx::query_as::<_, UserRow>(
        r#"
        SELECT * FROM users
        WHERE LOWER(username) = LOWER($1)
          AND (id = $2 OR suspended_until IS NULL OR suspended_until <= NOW())
        "#
    )
    .bind(&username)
    .bind(auth.user_id)
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
            let personalization_enabled = user.personalization_enabled;
            let mut response = UserPublicResponse::from(user);
            response.viewer_relationship = Some("self".to_string());
            response.is_admin = is_admin;
            response.email = Some(email);
            response.email_verified = Some(email_verified);
            response.personalization_enabled = Some(personalization_enabled);
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

    // While suspended, a profile can only be emptied, e.g. of the bio a
    // report was about (standing.rs lets this request through for that).
    // Forms send every field, so a field left as it is counts as untouched.
    let mut conn = state.db.acquire().await.db_err("Database error")?;
    if standing::suspension(&mut conn, auth.user_id).await?.is_some() && !only_clears(&input, &current_user) {
        return Err(AppError::forbidden(
            "Your account is suspended. You can only clear your display name or bio until the suspension ends.",
        ));
    }
    drop(conn);

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

    let mut tx = state.db.begin().await.db_err_ctx("Failed to start transaction", "Database error")?;

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
    .fetch_one(&mut *tx)
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

    // A profile under a likely-illegal report keeps a version per edit.
    let preserved = evidence::capture(&mut tx, "user", auth.user_id, evidence::Cause::Edited, Some(auth.user_id)).await?;
    tx.commit().await.db_err_ctx("Failed to commit transaction", "Database error")?;
    evidence::finish(&state, preserved, Vec::new()).await;

    Ok(Json(UserResponse::from(updated_user).resolve_media(&state.storage)))
}

/// Before an account goes: lowers the denormalized counters its likes,
/// comments and follows count in on other people's posts, comments and
/// profiles (they go with ON DELETE CASCADE, which leaves counters alone).
/// Its comments take their replies with them, so a post's comment_count
/// drops by the whole subtree -- minus comments removed by moderation,
/// which no longer count. Its own posts go entirely, so they're skipped.
async fn release_counters(tx: &mut sqlx::PgConnection, user_id: Uuid) -> Result<(), AppError> {
    let statements = [
        r#"
        UPDATE posts p SET like_count = GREATEST(p.like_count - 1, 0)
        FROM likes l WHERE l.post_id = p.id AND l.user_id = $1 AND p.user_id != $1
        "#,
        r#"
        UPDATE comments c SET like_count = GREATEST(c.like_count - 1, 0)
        FROM comment_likes cl WHERE cl.comment_id = c.id AND cl.user_id = $1 AND c.user_id != $1
        "#,
        r#"
        WITH RECURSIVE gone AS (
            SELECT c.id FROM comments c JOIN posts p ON p.id = c.post_id
            WHERE c.user_id = $1 AND p.user_id != $1
            UNION
            SELECT c.id FROM comments c JOIN gone g ON c.parent_comment_id = g.id
        )
        UPDATE posts p SET comment_count = GREATEST(p.comment_count - x.n, 0)
        FROM (
            SELECT c.post_id, COUNT(*) AS n FROM gone g JOIN comments c ON c.id = g.id
            WHERE c.moderation_status != 'removed' GROUP BY c.post_id
        ) x
        WHERE p.id = x.post_id
        "#,
        r#"
        UPDATE users u SET follower_count = GREATEST(u.follower_count - 1, 0)
        FROM follows f WHERE f.follower_id = $1 AND f.following_id = u.id
        "#,
        r#"
        UPDATE users u SET following_count = GREATEST(u.following_count - 1, 0)
        FROM follows f WHERE f.following_id = $1 AND f.follower_id = u.id
        "#,
    ];
    for sql in statements {
        sqlx::query(sql)
            .bind(user_id)
            .execute(&mut *tx)
            .await
            .db_err_ctx("Failed to update counters", "Failed to delete account")?;
    }
    Ok(())
}

/// Whether a profile update only empties fields or leaves them as they are.
fn only_clears(input: &UpdateProfileRequest, current: &UserRow) -> bool {
    let cleared_or_same = |new: &Option<String>, old: &Option<String>| match new {
        None => true,
        Some(value) => value.trim().is_empty() || Some(value.as_str()) == old.as_deref(),
    };
    input.username.as_deref().is_none_or(|u| u.trim() == current.username)
        && input.is_private.is_none_or(|p| p == current.is_private)
        && cleared_or_same(&input.display_name, &current.display_name)
        && cleared_or_same(&input.bio, &current.bio)
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

    let mut tx = state.db.begin().await.db_err_ctx("Failed to start transaction", "Database error")?;

    let old_avatar = sqlx::query_scalar::<_, Option<String>>(
        "SELECT avatar_url FROM users WHERE id = $1 FOR UPDATE"
    )
    .bind(auth.user_id)
    .fetch_one(&mut *tx)
    .await
    .db_err("Database error")?;

    let user = sqlx::query_as::<_, UserRow>(
        "UPDATE users SET avatar_url = $1 WHERE id = $2 RETURNING *"
    )
    .bind(&avatar_key)
    .bind(auth.user_id)
    .fetch_one(&mut *tx)
    .await
    .db_err("Failed to update avatar")?;

    // A profile under a likely-illegal report keeps the new picture as a
    // version; the old one was captured when it was reported.
    let preserved = evidence::capture(&mut tx, "user", auth.user_id, evidence::Cause::Edited, Some(auth.user_id)).await?;
    tx.commit().await.db_err_ctx("Failed to commit transaction", "Database error")?;

    // The old file goes only after the new one is in place, and stays while
    // a pending evidence copy still needs it (release_media).
    let old_key = old_avatar.map(|url| url.strip_prefix("/media/").unwrap_or(&url).to_string());
    evidence::finish(&state, preserved, old_key).await;

    tracing::info!("Avatar updated: {}", auth.user_id);
    Ok(Json(UserResponse::from(user).resolve_media(&state.storage)))
}

/// DELETE /users/me/avatar — removes the profile picture. Also open to a
/// suspended account, which can't otherwise change its profile, so it can
/// take down a picture a report was about.
pub async fn delete_avatar(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<UserResponse>, AppError> {
    let mut tx = state.db.begin().await.db_err_ctx("Failed to start transaction", "Database error")?;
    let old_avatar = sqlx::query_scalar::<_, Option<String>>("SELECT avatar_url FROM users WHERE id = $1 FOR UPDATE")
        .bind(auth.user_id)
        .fetch_one(&mut *tx)
        .await
        .db_err("Database error")?;
    let user = sqlx::query_as::<_, UserRow>("UPDATE users SET avatar_url = NULL WHERE id = $1 RETURNING *")
        .bind(auth.user_id)
        .fetch_one(&mut *tx)
        .await
        .db_err("Failed to remove avatar")?;
    // A profile under a likely-illegal report keeps the change as a
    // version; the picture itself was captured when it was reported.
    let preserved = evidence::capture(&mut tx, "user", auth.user_id, evidence::Cause::Edited, Some(auth.user_id)).await?;
    tx.commit().await.db_err_ctx("Failed to commit transaction", "Database error")?;

    let old_key = old_avatar.map(|url| url.strip_prefix("/media/").unwrap_or(&url).to_string());
    evidence::finish(&state, preserved, old_key).await;
    tracing::info!("Avatar removed: {}", auth.user_id);
    Ok(Json(UserResponse::from(user).resolve_media(&state.storage)))
}


#[derive(Debug, Deserialize)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

/// PATCH /users/me/password — change password (auth required)
/// Requires current password to verify identity before updating. Ends every
/// session at once -- whoever knew the old password may be signed in
/// somewhere -- and returns fresh tokens, so only this device stays signed
/// in.
pub async fn change_password(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<ChangePasswordRequest>,
) -> Result<(axum::http::HeaderMap, Json<RefreshResponse>), AppError> {

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

    let mut tx = state.db.begin().await.db_err_ctx("Failed to start transaction", "Database error")?;
    sqlx::query("UPDATE users SET password_hash = $1 WHERE id = $2")
        .bind(&new_hash)
        .bind(auth.user_id)
        .execute(&mut *tx)
        .await
        .db_err("Failed to update password")?;
    crate::handlers::account_lock::revoke_sessions(&mut tx, auth.user_id).await?;
    tx.commit().await.db_err_ctx("Failed to commit transaction", "Database error")?;

    let (cookies, access_token, refresh_token) = crate::handlers::auth::new_session(&state, auth.user_id).await?;
    tracing::info!("Password changed for user: {}", auth.user_id);
    Ok((cookies, Json(RefreshResponse { access_token, refresh_token })))
}

#[derive(Debug, Deserialize)]
pub struct DeleteAccountRequest {
    pub password: String,
}

/// DELETE /users/me — delete account and all associated data (auth
/// required). Asks for the password: a session alone -- an unlocked phone,
/// a stolen access token -- mustn't be enough to erase everything for good.
pub async fn delete_account(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<DeleteAccountRequest>,
) -> Result<StatusCode, AppError> {
    let user = sqlx::query_as::<_, UserRow>("SELECT * FROM users WHERE id = $1")
        .bind(auth.user_id)
        .fetch_one(&state.db)
        .await
        .db_err("Database error")?;
    crate::handlers::auth::verify_password(&user, &input.password)
        .map_err(|_| AppError::bad_request("The password is incorrect"))?;
    delete_user(&state, auth.user_id, Deletion::Owner(auth.user_id)).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Who deletes an account.
#[derive(Clone, Copy, Debug)]
pub enum Deletion {
    /// Its owner, DELETE /users/me.
    Owner(Uuid),
    /// The standing sweeper, once a permanent suspension's objection window
    /// has passed (standing.rs).
    Ban,
    /// The retention sweeper: the address was never verified
    /// (retention.rs). Checked again in the deleting transaction, in case
    /// it was verified just now; then nothing is deleted.
    Unverified,
}

/// Deletes an account and all associated data; see `Deletion` for who.
///
/// Deletion order:
/// 1. Fetch all media file keys for this user's posts (before CASCADE removes them)
/// 2. Fetch avatar key
/// 3. Delete the user record (CASCADE handles all DB relations)
/// 4. Delete media files from disk
pub async fn delete_user(state: &AppState, user_id: Uuid, by: Deletion) -> Result<(), AppError> {
    let actor = match by {
        Deletion::Owner(id) => Some(id),
        Deletion::Ban | Deletion::Unverified => None,
    };

    // Collect all media file keys for this user's posts
    let media_keys = sqlx::query_as::<_, (String, String, String)>(
        r#"
        SELECT ma.thumb_key, ma.medium_key, ma.full_key
        FROM media_assets ma
        JOIN posts p ON ma.post_id = p.id
        WHERE p.user_id = $1
        "#
    )
    .bind(user_id)
    .fetch_all(&state.db)
    .await
    .db_err("Database error")?;

    // Get avatar key
    let avatar_url = sqlx::query_scalar::<_, Option<String>>(
        "SELECT avatar_url FROM users WHERE id = $1"
    )
    .bind(user_id)
    .fetch_one(&state.db)
    .await
    .db_err("Database error")?;

    let mut tx = state.db.begin().await.db_err_ctx("Failed to start transaction", "Database error")?;

    let verified = sqlx::query_scalar::<_, bool>("SELECT email_verified FROM users WHERE id = $1 FOR UPDATE")
        .bind(user_id)
        .fetch_optional(&mut *tx)
        .await
        .db_err("Database error")?
        .ok_or_else(|| AppError::not_found("User not found"))?;
    if matches!(by, Deletion::Unverified) && verified {
        return Ok(());
    }

    // The deletion goes ahead (Art. 17), but content with a pending
    // likely-illegal report -- this user's posts and comments, comments
    // under their posts, and the profile itself -- and every reported
    // message they sent is preserved as evidence first (Art. 17(3)(e));
    // see evidence.rs.
    let scope = evidence::Scope {
        posts: sqlx::query_scalar::<_, Uuid>("SELECT id FROM posts WHERE user_id = $1")
            .bind(user_id)
            .fetch_all(&mut *tx)
            .await
            .db_err_ctx("Failed to list posts", "Failed to delete account")?,
        comments: sqlx::query_scalar::<_, Uuid>("SELECT id FROM comments WHERE user_id = $1")
            .bind(user_id)
            .fetch_all(&mut *tx)
            .await
            .db_err_ctx("Failed to list comments", "Failed to delete account")?,
        // Only reported messages are preserved, so only those are listed:
        // years of chats would make a long list for nothing.
        messages: sqlx::query_scalar::<_, Uuid>(
            r#"
            SELECT m.id FROM messages m
            WHERE m.sender_id = $1 AND EXISTS (
                SELECT 1 FROM reports r WHERE r.target_type = 'message' AND r.target_id = m.id AND r.status = 'pending'
            )
            "#,
        )
        .bind(user_id)
        .fetch_all(&mut *tx)
        .await
        .db_err_ctx("Failed to list reported messages", "Failed to delete account")?,
        user: Some(user_id),
    };
    let preserved = evidence::preserve(&mut tx, &scope, evidence::Trigger::AccountDeletion, actor).await?;

    // Reports on what goes with the account that preserved nothing have
    // nothing left to decide; their reporters learn it's gone. That
    // includes other people's comments under this user's posts.
    let comments_gone = sqlx::query_scalar::<_, Uuid>(
        "SELECT c.id FROM comments c JOIN posts p ON p.id = c.post_id WHERE c.user_id = $1 OR p.user_id = $1",
    )
    .bind(user_id)
    .fetch_all(&mut *tx)
    .await
    .db_err_ctx("Failed to list comments", "Failed to delete account")?;
    let mut notices = moderation::close_obsolete_reports(&mut tx, "post", &scope.posts).await?;
    notices.extend(moderation::close_obsolete_reports(&mut tx, "comment", &comments_gone).await?);
    notices.extend(moderation::close_obsolete_reports(&mut tx, "user", &[user_id]).await?);

    // Decision records about this account stay as the moderation audit
    // trail, without the content excerpt.
    moderation::forget_user(&mut tx, user_id).await?;

    // Feedback text stays (it's about the app, see the feedback migration),
    // but screenshots may show this person, so they go with the account.
    let screenshot_keys = crate::handlers::feedback::forget_screenshots(&mut tx, user_id).await?;

    // Conversations whose other participant already deleted their account
    // would be left with nobody in them once this user goes too -- remove
    // them outright. Conversations with a remaining participant are kept
    // for that person; this user's side becomes NULL ("Deleted User") via
    // ON DELETE SET NULL (migration 20260929000100), and this user's
    // messages in it are erased via ON DELETE CASCADE (20260930000000).
    sqlx::query(
        "DELETE FROM conversations WHERE (user1_id = $1 AND user2_id IS NULL) OR (user2_id = $1 AND user1_id IS NULL)"
    )
    .bind(user_id)
    .execute(&mut *tx)
    .await
    .db_err_ctx("Failed to delete orphaned conversations", "Failed to delete account")?;

    // The cascade below also removes this user's likes, comments and
    // follows on other people's things; their counters follow here.
    release_counters(&mut tx, user_id).await?;

    // Delete user — CASCADE removes posts, comments, likes, follows, blocks,
    // sent messages, refresh_tokens, email_tokens, media_asset rows
    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(user_id)
        .execute(&mut *tx)
        .await
        .db_err("Failed to delete account")?;

    tx.commit().await.db_err_ctx("Failed to commit transaction", "Failed to delete account")?;
    notices.send(state).await;

    // Clean up the files and their CDN copies (best-effort, failures are
    // logged by delete_media), apart from preserved ones still waiting
    // for their evidence copy.
    let avatar_key = avatar_url.map(|url| url.strip_prefix("/media/").unwrap_or(&url).to_string());
    let keys = media_keys
        .into_iter()
        .flat_map(|(thumb, medium, full)| [thumb, medium, full])
        .chain(avatar_key)
        .chain(screenshot_keys);
    evidence::finish(state, preserved, keys).await;

    tracing::info!("Account deleted: {} ({:?})", user_id, by);
    Ok(())
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
    #[allow(clippy::type_complexity)]
    let profile = sqlx::query_as::<_, (String, String, Option<String>, Option<String>, Option<String>, bool, DateTime<Utc>, Option<DateTime<Utc>>, Option<DateTime<Utc>>, bool)>(
        "SELECT username, email, display_name, bio, avatar_url, email_verified, created_at, terms_accepted_at, keep_after_test_at, personalization_enabled FROM users WHERE id = $1"
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

    // --- Interaction log for ranking Discovery (handlers/events.rs) ---
    let post_events = sqlx::query_as::<_, (Uuid, String, DateTime<Utc>)>(
        "SELECT post_id, event_type, created_at FROM post_events WHERE user_id = $1 ORDER BY created_at DESC"
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

    // --- Follow requests still pending, both ways ---
    let requests_sent = sqlx::query_as::<_, (String, DateTime<Utc>)>(
        "SELECT u.username, r.created_at FROM follow_requests r JOIN users u ON u.id = r.target_id WHERE r.requester_id = $1 ORDER BY r.created_at DESC"
    )
    .bind(auth.user_id)
    .fetch_all(&state.db)
    .await
    .db_err_ctx("Data export query failed", "Database error")?;

    let requests_received = sqlx::query_as::<_, (String, DateTime<Utc>)>(
        "SELECT u.username, r.created_at FROM follow_requests r JOIN users u ON u.id = r.requester_id WHERE r.target_id = $1 ORDER BY r.created_at DESC"
    )
    .bind(auth.user_id)
    .fetch_all(&state.db)
    .await
    .db_err_ctx("Data export query failed", "Database error")?;

    // --- Changed Terms and privacy policy: what this account was shown,
    // and when it accepted or acknowledged it ---
    let legal_acks = sqlx::query_as::<_, (Vec<String>, DateTime<Utc>, DateTime<Utc>, bool)>(
        r#"
        SELECT l.documents, l.published_at, a.acknowledged_at, a.accepted
        FROM legal_update_acks a JOIN legal_updates l ON l.id = a.update_id
        WHERE a.user_id = $1 ORDER BY a.acknowledged_at
        "#
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

    // --- Feedback sent through the app ---
    let (feedback_sent, feedback_files) = crate::handlers::feedback::export_for(&state.db, auth.user_id).await?;
    export_files.extend(feedback_files);

    // --- Notifications received (capped — this is a personal export, not
    // an unbounded audit log) ---
    // LEFT JOIN: moderation notices come from Klar, with no acting user.
    let notifications = sqlx::query_as::<_, (String, Option<String>, Option<Uuid>, Option<Uuid>, bool, DateTime<Utc>)>(
        r#"
        SELECT n.type::text, u.username, n.post_id, n.comment_id, n.is_read, n.created_at
        FROM notifications n LEFT JOIN users u ON u.id = n.actor_id
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
            "from": actor_username.unwrap_or_else(|| "Klar".to_string()),
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

    // Statements of reasons about this account's content and the reports
    // it filed. Preserved evidence is deliberately not included (see
    // evidence.rs).
    let mut conn = state.db.acquire().await.db_err("Database error")?;
    let moderation_json = moderation::export_for(&mut conn, auth.user_id).await?;
    drop(conn);

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
            "keep_after_test_at": profile.8,
            "personalization_enabled": profile.9,
        },
        "posts": posts_json,
        "comments": comments_json,
        "likes_given": {
            "posts": post_likes.into_iter().map(|(post_id, created_at)| serde_json::json!({"post_id": post_id, "created_at": created_at})).collect::<Vec<_>>(),
            "comments": comment_likes.into_iter().map(|(comment_id, created_at)| serde_json::json!({"comment_id": comment_id, "created_at": created_at})).collect::<Vec<_>>(),
        },
        "discovery_interactions": post_events.into_iter().map(|(post_id, event_type, created_at)| serde_json::json!({"post_id": post_id, "event": event_type, "created_at": created_at})).collect::<Vec<_>>(),
        "following": following.into_iter().map(|(username, since)| serde_json::json!({"username": username, "since": since})).collect::<Vec<_>>(),
        "followers": followers.into_iter().map(|(username, since)| serde_json::json!({"username": username, "since": since})).collect::<Vec<_>>(),
        "blocked_users": blocked.into_iter().map(|(username, since)| serde_json::json!({"username": username, "since": since})).collect::<Vec<_>>(),
        "follow_requests": {
            "sent": requests_sent.into_iter().map(|(username, since)| serde_json::json!({"username": username, "since": since})).collect::<Vec<_>>(),
            "received": requests_received.into_iter().map(|(username, since)| serde_json::json!({"username": username, "since": since})).collect::<Vec<_>>(),
        },
        "legal_updates": legal_acks.into_iter().map(|(documents, published_at, acknowledged_at, accepted)| serde_json::json!({
            "documents": documents,
            "published_at": published_at,
            "acknowledged_at": acknowledged_at,
            "accepted": accepted,
        })).collect::<Vec<_>>(),
        "feedback_sent": feedback_sent,
        "notifications_received": notifications_json,
        "conversations": conversations_json,
        "moderation": moderation_json,
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
