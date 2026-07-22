/// Upload handler — multipart image upload, processing, and post creation.
///
/// Flow:
/// 1. Client sends multipart form: caption (text) + image (file)
/// 2. Server validates the image (type, size)
/// 3. Server processes: strip EXIF by re-encoding, generate 3 variants
/// 4. Server saves variants to local storage
/// 5. Server creates post + media_asset records in DB, fans out to
///    followers' feed_items, and bumps the author's post_count — all in
///    one transaction, matching handlers::posts::create_post
/// 6. Server returns the post with media URLs

use axum::{
    extract::{Multipart, Path, State},
    http::StatusCode,
    Json,
};
use serde::Serialize;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::errors::AppError;
use crate::handlers::auth::AppState;
use crate::media;
use crate::models::{MediaAsset, NewPostResponse};

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
    .map_err(|e| AppError::internal(format!("Processing task failed: {}", e)))?
    .map_err(|e| AppError::bad_request(format!("Image processing failed: {}", e)))?;

    // Generate a unique ID for this media asset's files
    let media_id = Uuid::new_v4();
    let ext = "webp"; // We always re-encode to WebP

    // Save variants to storage
    let thumb_key = format!("thumb/{}.{}", media_id, ext);
    let medium_key = format!("medium/{}.{}", media_id, ext);
    let full_key = format!("full/{}.{}", media_id, ext);

    state.storage.save(&thumb_key, &processed.thumb).await
        .map_err(|e| AppError::internal(format!("Failed to save thumbnail: {:?}", e)))?;
    state.storage.save(&medium_key, &processed.medium).await
        .map_err(|e| AppError::internal(format!("Failed to save medium: {:?}", e)))?;
    state.storage.save(&full_key, &processed.full).await
        .map_err(|e| AppError::internal(format!("Failed to save full image: {:?}", e)))?;

    // Everything from here on is one transaction — post + media_asset +
    // post_count + feed fan-out all succeed together or not at all.
    // Previously these ran as separate un-transacted queries against
    // state.db directly, and the fan-out/post_count steps were simply
    // missing entirely — this is why photo posts (the only kind real
    // users create) never showed up in followers' feeds.
    let mut tx = state.db.begin().await.map_err(|e| {
        tracing::error!("Failed to start transaction: {}", e);
        AppError::internal("Database error")
    })?;

    // Create post in database
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
    .bind(auth.user_id)
    .bind(&caption)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!("Failed to create post: {}", e);
        AppError::internal("Failed to create post")
    })?;

    // Build public URLs
    let thumb_url = state.storage.public_url(&thumb_key);
    let medium_url = state.storage.public_url(&medium_key);
    let full_url = state.storage.public_url(&full_key);

    // Create media asset record
    let media_asset = sqlx::query_as::<_, MediaAsset>(
        r#"
        INSERT INTO media_assets (post_id, original_key, thumb_key, medium_key, full_key, width, height, size_bytes)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        RETURNING
            id,
            post_id,
            $9 as thumb_url,
            $10 as medium_url,
            $11 as full_url,
            width,
            height,
            size_bytes,
            created_at
        "#
    )
    .bind(post.id)
    .bind(&full_key) // original_key — we use full as the "original" since we strip EXIF
    .bind(&thumb_key)
    .bind(&medium_key)
    .bind(&full_key)
    .bind(processed.width as i32)
    .bind(processed.height as i32)
    .bind(processed.medium.len() as i64) // approximate size
    .bind(&thumb_url)
    .bind(&medium_url)
    .bind(&full_url)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!("Failed to create media asset: {}", e);
        AppError::internal("Failed to save media record")
    })?;

    // Keep the author's denormalized post_count in sync (create_post does
    // this too; this handler was missing it entirely before)
    sqlx::query("UPDATE users SET post_count = post_count + 1 WHERE id = $1")
        .bind(auth.user_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| { tracing::error!("Failed to update post_count: {}", e); AppError::internal("Database error") })?;

    // Fan-out: one feed_items row per current follower, same as
    // handlers::posts::create_post — this was the missing piece that made
    // photo posts never appear in followers' feeds.
    sqlx::query(
        r#"
        INSERT INTO feed_items (user_id, post_id, created_at)
        SELECT follower_id, $1, $2 FROM follows WHERE following_id = $3
        ON CONFLICT DO NOTHING
        "#
    )
    .bind(post.id)
    .bind(post.created_at)
    .bind(auth.user_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| { tracing::error!("Failed to fan out post: {}", e); AppError::internal("Database error") })?;

    tx.commit().await.map_err(|e| {
        tracing::error!("Failed to commit transaction: {}", e);
        AppError::internal("Database error")
    })?;

    tracing::info!("Post with media created: {} by user {}", post.id, auth.user_id);

    Ok((
        StatusCode::CREATED,
        Json(PostWithMediaResponse {
            post,
            media: vec![media_asset],
        }),
    ))
}

/// GET /posts/:id/media — get media assets for a post
pub async fn get_post_media(
    State(state): State<AppState>,
    Path(post_id): Path<Uuid>,
) -> Result<Json<Vec<MediaAsset>>, AppError> {

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
    .map_err(|e| {
        tracing::error!("Database error: {}", e);
        AppError::internal("Database error")
    })?;

    // Convert storage keys to public URLs
    let assets_with_urls: Vec<MediaAsset> = assets.into_iter().map(|mut a| {
        a.thumb_url = state.storage.public_url(&a.thumb_url);
        a.medium_url = state.storage.public_url(&a.medium_url);
        a.full_url = state.storage.public_url(&a.full_url);
        a
    }).collect();

    Ok(Json(assets_with_urls))
}
