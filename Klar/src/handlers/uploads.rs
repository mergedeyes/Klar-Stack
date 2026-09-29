//! Upload handler — multipart image upload, processing, and post creation.
//!
//! Flow:
//! 1. Client sends multipart form: caption (text) + image (file)
//! 2. Server validates the image (type, size)
//! 3. Server processes: strip EXIF by re-encoding, generate 3 variants
//! 4. Server saves variants to local storage
//! 5. Server creates post + media_asset records in DB, fans out to
//!    followers' feed_items, and bumps the author's post_count — all in
//!    one transaction, matching handlers::posts::create_post
//! 6. Server returns the post with media URLs

use axum::{
    extract::{Multipart, Path, State},
    http::StatusCode,
    Json,
};
use serde::Serialize;
use uuid::Uuid;

use crate::auth::{AuthUser, OptionalAuthUser};
use crate::handlers::posts::{record_new_post, require_visible_post};
use crate::errors::AppError;
use crate::handlers::auth::AppState;
use crate::media;
use crate::models::{MediaAsset, NewPostResponse};
use crate::utils::{delete_media, DbResultExt, ResolveMedia};
use crate::validation::{check_max_len, CAPTION_MAX};

/// Combined response for a post with its media
#[derive(Debug, Serialize)]
pub struct PostWithMediaResponse {
    pub post: NewPostResponse,
    pub media: Vec<MediaAsset>,
}

/// Maximum upload size: 20MB
const MAX_FILE_SIZE: usize = 20 * 1024 * 1024;

/// Allowed MIME types
const ALLOWED_TYPES: &[&str] = &["image/jpeg", "image/png", "image/webp"];

/// POST /posts/upload — create a post with an image (auth required)
/// Expects multipart/form-data with fields:
///   - "caption" (optional text field)
///   - "image" (required file field)
pub async fn upload_post(
    State(state): State<AppState>,
    auth: AuthUser,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<PostWithMediaResponse>), AppError> {

    let mut caption: Option<String> = None;
    let mut image_data: Option<Vec<u8>> = None;
    let mut content_type: Option<String> = None;

    // Parse multipart fields
    while let Some(field) = multipart.next_field().await
        .map_err(|e| AppError::bad_request(format!("Invalid multipart data: {}", e)))?
    {
        let name = field.name().unwrap_or("").to_string();

        match name.as_str() {
            "caption" => {
                caption = Some(
                    field.text().await
                        .map_err(|e| AppError::bad_request(format!("Failed to read caption: {}", e)))?
                );
            }
            "image" => {
                // Get content type before consuming the field
                content_type = field.content_type().map(|s| s.to_string());

                let bytes = field.bytes().await
                    .map_err(|e| AppError::bad_request(format!("Failed to read image: {}", e)))?;

                if bytes.len() > MAX_FILE_SIZE {
                    return Err(AppError::bad_request("Image must be under 20MB"));
                }

                if bytes.is_empty() {
                    return Err(AppError::bad_request("Image file is empty"));
                }

                image_data = Some(bytes.to_vec());
            }
            _ => {
                // Ignore unknown fields
            }
        }
    }

    // Validate we got an image
    let raw_bytes = image_data.ok_or_else(|| AppError::bad_request("Image field is required"))?;

    // Caption is optional for image posts; an all-whitespace one is stored
    // as no caption rather than as blank text.
    let caption = caption
        .map(|c| c.trim().to_string())
        .filter(|c| !c.is_empty());
    if let Some(c) = &caption {
        check_max_len(c, "Caption", CAPTION_MAX)?;
    }

    // Validate content type
    if let Some(ref ct) = content_type {
        if !ALLOWED_TYPES.contains(&ct.as_str()) {
            return Err(AppError::bad_request(
                format!("Unsupported image type: {}. Allowed: JPEG, PNG, WebP", ct)
            ));
        }
    }

    // Process the image — resize, strip EXIF, generate variants
    // This is synchronous and CPU-bound, so we run it in a blocking task
    // to avoid blocking the async runtime
    let processed = tokio::task::spawn_blocking(move || {
        media::process_image(&raw_bytes)
    })
    .await
    // A JoinError means the task panicked or was cancelled -- log the
    // detail, but don't echo a panic message to the client.
    .map_err(|e| {
        tracing::error!("Image processing task failed: {}", e);
        AppError::internal("Image processing failed")
    })?
    .map_err(|e| AppError::bad_request(format!("Image processing failed: {}", e)))?;

    // Generate a unique ID for this media asset's files
    let media_id = Uuid::new_v4();
    let ext = "webp"; // We always re-encode to WebP
    let thumb_key = format!("thumb/{}.{}", media_id, ext);
    let medium_key = format!("medium/{}.{}", media_id, ext);
    let full_key = format!("full/{}.{}", media_id, ext);

    // If a later save or the DB transaction fails, the files already
    // written are deleted again -- otherwise they'd sit in storage with no
    // row pointing at them: a copy of someone's photo that no deletion
    // path (post, account, moderation) could ever reach.
    let mut saved: Vec<&str> = Vec::new();
    let result = async {
        for (key, data) in [
            (&thumb_key, &processed.thumb),
            (&medium_key, &processed.medium),
            (&full_key, &processed.full),
        ] {
            state.storage.save(key, data).await?;
            saved.push(key);
        }
        insert_post_with_media(&state, auth.user_id, caption.as_deref(), &processed, [&thumb_key, &medium_key, &full_key]).await
    }
    .await;

    let (post, media_asset) = match result {
        Ok(created) => created,
        Err(e) => {
            for key in saved {
                delete_media(&state, key).await;
            }
            return Err(e);
        }
    };

    tracing::info!("Post with media created: {} by user {}", post.id, auth.user_id);

    Ok((
        StatusCode::CREATED,
        Json(PostWithMediaResponse {
            post,
            media: vec![media_asset.resolve_media(&state.storage)],
        }),
    ))
}

/// Insert the post and its media_asset row, plus the bookkeeping every new
/// post needs (post_count, feed fan-out), in one transaction -- all of it
/// succeeds together or not at all.
async fn insert_post_with_media(
    state: &AppState,
    author_id: Uuid,
    caption: Option<&str>,
    processed: &media::ProcessedImage,
    [thumb_key, medium_key, full_key]: [&str; 3],
) -> Result<(NewPostResponse, MediaAsset), AppError> {
    let mut tx = state.db.begin().await.db_err_ctx("Failed to start transaction", "Database error")?;

    let post = sqlx::query_as::<_, NewPostResponse>(
        r#"
        INSERT INTO posts (user_id, caption)
        VALUES ($1, $2)
        RETURNING
            id,
            user_id,
            (SELECT username FROM users WHERE id = $1) as username,
            (SELECT avatar_url FROM users WHERE id = $1) as avatar_url,
            caption,
            created_at,
            edited_at
        "#
    )
    .bind(author_id)
    .bind(caption)
    .fetch_one(&mut *tx)
    .await
    .db_err("Failed to create post")?;

    // Returns the keys aliased as thumb_url/medium_url/full_url -- still
    // bare storage keys here, resolved by the caller via .resolve_media(),
    // same as every other handler that reads media_assets.
    let media_asset = sqlx::query_as::<_, MediaAsset>(
        r#"
        INSERT INTO media_assets (post_id, original_key, thumb_key, medium_key, full_key, width, height, size_bytes)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        RETURNING
            id,
            post_id,
            thumb_key as thumb_url,
            medium_key as medium_url,
            full_key as full_url,
            width,
            height,
            size_bytes,
            created_at
        "#
    )
    .bind(post.id)
    .bind(full_key) // original_key — we use full as the "original" since we strip EXIF
    .bind(thumb_key)
    .bind(medium_key)
    .bind(full_key)
    .bind(processed.width as i32)
    .bind(processed.height as i32)
    .bind(processed.medium.len() as i64) // approximate size
    .fetch_one(&mut *tx)
    .await
    .db_err_ctx("Failed to create media asset", "Failed to save media record")?;

    record_new_post(&mut tx, author_id, post.id, post.created_at).await?;

    tx.commit().await.db_err_ctx("Failed to commit transaction", "Database error")?;

    Ok((post, media_asset))
}

/// GET /posts/:id/media — get media assets for a post
pub async fn get_post_media(
    State(state): State<AppState>,
    auth: OptionalAuthUser,
    Path(post_id): Path<Uuid>,
) -> Result<Json<Vec<MediaAsset>>, AppError> {

    require_visible_post(&state.db, auth.user_id, post_id).await?;

    let assets = sqlx::query_as::<_, MediaAsset>(
        r#"
        SELECT
            id,
            post_id,
            thumb_key as thumb_url,
            medium_key as medium_url,
            full_key as full_url,
            width,
            height,
            size_bytes,
            created_at
        FROM media_assets
        WHERE post_id = $1
        ORDER BY sort_order
        "#
    )
    .bind(post_id)
    .fetch_all(&state.db)
    .await
    .db_err("Database error")?;

    Ok(Json(assets.resolve_media(&state.storage)))
}
