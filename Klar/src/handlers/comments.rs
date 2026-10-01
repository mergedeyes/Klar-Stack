//! Comment handlers — create, list, edit, and delete comments on posts.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use uuid::Uuid;

use crate::auth::{AuthUser, OptionalAuthUser};
use crate::errors::AppError;
use crate::evidence;
use crate::handlers::auth::AppState;
use crate::handlers::blocks::check_block;
use crate::handlers::events::{record_event, EventType};
use crate::handlers::posts::require_visible_post;
use crate::handlers::notifications::{insert_notification, publish_notification, NotificationKind};
use crate::models::{CommentResponse, CreateCommentRequest, EditCommentRequest};
use crate::moderation;
use crate::utils::{DbResultExt, ResolveMedia};
use crate::validation::{required_text, COMMENT_MAX};

/// posts.comment_count is maintained here (create/delete) instead of a
/// correlated COUNT(*) subquery per post on every feed/profile render.
pub async fn create_comment(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(post_id): Path<Uuid>,
    Json(input): Json<CreateCommentRequest>,
) -> Result<(StatusCode, Json<CommentResponse>), AppError> {

    let body = required_text(&input.body, "Comment", COMMENT_MAX)?;

    let post_owner = require_visible_post(&state.db, Some(auth.user_id), post_id).await?;

    if check_block(&state.db, auth.user_id, post_owner).await? {
        return Err(AppError::bad_request("Cannot comment on this post"));
    }

    if let Some(parent_id) = input.parent_comment_id {
        let parent_exists = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM comments WHERE id = $1 AND post_id = $2 AND moderation_status != 'removed')"
        )
        .bind(parent_id)
        .bind(post_id)
        .fetch_one(&state.db)
        .await
        .db_err("Database error")?;

        if !parent_exists {
            return Err(AppError::not_found("Parent comment not found on this post"));
        }
    }

    let mut tx = state.db.begin().await.db_err_ctx("Failed to start transaction", "Database error")?;

    let comment = sqlx::query_as::<_, CommentResponse>(
        r#"
        INSERT INTO comments (post_id, user_id, parent_comment_id, body)
        VALUES ($1, $2, $3, $4)
        RETURNING
            id, post_id, user_id,
            (SELECT username FROM users WHERE id = $2) as username,
            (SELECT avatar_url FROM users WHERE id = $2) as avatar_url,
            parent_comment_id, body, created_at, edited_at,
            0::bigint as like_count,
            false as liked,
            moderation_status::text
        "#
    )
    .bind(post_id)
    .bind(auth.user_id)
    .bind(input.parent_comment_id)
    .bind(body)
    .fetch_one(&mut *tx)
    .await
    .db_err("Failed to create comment")?;

    sqlx::query("UPDATE posts SET comment_count = comment_count + 1 WHERE id = $1")
        .bind(post_id)
        .execute(&mut *tx)
        .await
        .db_err_ctx("Failed to update comment_count", "Database error")?;

    // Notify the post owner, unless they're commenting on their own post.
    // Built inside the transaction (needs the notification id + actor
    // row + post thumbnail), published to Redis only after commit -- same
    // reasoning as likes.rs/follows.rs.
    let pending_notification = insert_notification(
        &mut tx, post_owner, auth.user_id, NotificationKind::Comment, Some(post_id), Some(comment.id),
    ).await;

    tx.commit().await.db_err_ctx("Failed to commit transaction", "Database error")?;

    if let Some(event) = pending_notification {
        publish_notification(&state, &event).await;
    }

    record_event(&state.db, auth.user_id, post_id, EventType::Comment).await;

    Ok((StatusCode::CREATED, Json(comment.resolve_media(&state.storage))))
}

/// GET /posts/:id/comments — list comments on a post.
///
/// Excludes "hidden" comments (auto-hidden via a CSAM report) for
/// everyone except the comment's own author, same pattern as
/// posts::get_post -- so a hidden comment's content isn't distinguishable
/// from simply not existing to anyone but the person who wrote it.
/// "flagged" comments (lower-severity reports) still appear here; the
/// interstitial warning is a frontend concern. A comment removed by the
/// moderation team only appears when others replied to it, as an empty
/// placeholder ("removed"), so their replies keep their place.
pub async fn get_comments(
    State(state): State<AppState>,
    auth: OptionalAuthUser,
    Path(post_id): Path<Uuid>,
) -> Result<Json<Vec<CommentResponse>>, AppError> {

    let user_id = auth.user_id;

    require_visible_post(&state.db, user_id, post_id).await?;

    let comments = sqlx::query_as::<_, CommentResponse>(
        r#"
        SELECT
            c.id, c.post_id, c.user_id, u.username, u.avatar_url,
            c.parent_comment_id,
            CASE WHEN c.moderation_status = 'removed' THEN '' ELSE c.body END AS body,
            c.created_at, c.edited_at,
            c.like_count,
            CASE
                WHEN $2::uuid IS NULL THEN false
                ELSE EXISTS(
                    SELECT 1 FROM comment_likes
                    WHERE comment_id = c.id AND user_id = $2::uuid
                )
            END AS liked,
            c.moderation_status::text
        FROM comments c
        JOIN users u ON c.user_id = u.id
        WHERE c.post_id = $1
            AND (c.moderation_status != 'hidden' OR c.user_id = $2)
            AND (c.moderation_status != 'removed'
                 OR EXISTS (SELECT 1 FROM comments r WHERE r.parent_comment_id = c.id))
            -- A suspended author's comments are hidden (standing.rs).
            AND (c.user_id = $2 OR (u.suspended_until IS NULL OR u.suspended_until <= NOW()))
        ORDER BY c.created_at ASC
        "#
    )
    .bind(post_id)
    .bind(user_id)
    .fetch_all(&state.db)
    .await
    .db_err("Database error")?;

    Ok(Json(comments.resolve_media(&state.storage)))
}

pub async fn edit_comment(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((post_id, comment_id)): Path<(Uuid, Uuid)>,
    Json(input): Json<EditCommentRequest>,
) -> Result<Json<CommentResponse>, AppError> {

    let body = required_text(&input.body, "Comment", COMMENT_MAX)?;

    // A removed comment is kept, unseen, only so that a successful
    // objection can restore it as it was.
    let comment_author = sqlx::query_scalar::<_, Uuid>(
        "SELECT user_id FROM comments WHERE id = $1 AND post_id = $2 AND moderation_status != 'removed'"
    )
    .bind(comment_id)
    .bind(post_id)
    .fetch_optional(&state.db)
    .await
    .db_err("Database error")?
    .ok_or_else(|| AppError::not_found("Comment not found"))?;

    if comment_author != auth.user_id {
        return Err(AppError::forbidden("You can only edit your own comments"));
    }

    let mut tx = state.db.begin().await.db_err_ctx("Failed to start transaction", "Database error")?;

    let comment = sqlx::query_as::<_, CommentResponse>(
        r#"
        UPDATE comments SET body = $1, edited_at = NOW()
        WHERE id = $2
        RETURNING
            id, post_id, user_id,
            (SELECT username FROM users WHERE id = user_id) as username,
            (SELECT avatar_url FROM users WHERE id = user_id) as avatar_url,
            parent_comment_id, body, created_at, edited_at,
            like_count,
            false as liked,
            moderation_status::text
        "#
    )
    .bind(body)
    .bind(comment_id)
    .fetch_one(&mut *tx)
    .await
    .db_err("Failed to edit comment")?;

    // A comment under a likely-illegal report keeps a version per edit.
    let preserved = evidence::capture(&mut tx, "comment", comment_id, evidence::Cause::Edited, Some(auth.user_id)).await?;
    tx.commit().await.db_err_ctx("Failed to commit transaction", "Database error")?;
    evidence::finish(&state, preserved, Vec::new()).await;

    Ok(Json(comment.resolve_media(&state.storage)))
}

/// posts.comment_count is decremented here to match create_comment's increment.
pub async fn delete_comment(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((post_id, comment_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {

    let comment = sqlx::query_as::<_, (Uuid, Uuid)>(
        "SELECT user_id, post_id FROM comments WHERE id = $1 AND post_id = $2"
    )
    .bind(comment_id)
    .bind(post_id)
    .fetch_optional(&state.db)
    .await
    .db_err("Database error")?;

    let (comment_author, _) = comment
        .ok_or_else(|| AppError::not_found("Comment not found"))?;

    let post_owner = sqlx::query_scalar::<_, Uuid>(
        "SELECT user_id FROM posts WHERE id = $1"
    )
    .bind(post_id)
    .fetch_one(&state.db)
    .await
    .db_err("Database error")?;

    if auth.user_id != comment_author && auth.user_id != post_owner {
        return Err(AppError::forbidden(
            "You can only delete your own comments or comments on your posts",
        ));
    }

    let mut tx = state.db.begin().await.db_err_ctx("Failed to start transaction", "Database error")?;

    // A reported comment (or reply under it) is preserved as evidence
    // before it goes; see evidence.rs.
    let scope = evidence::Scope { comments: vec![comment_id], ..Default::default() };
    let preserved = evidence::preserve(&mut tx, &scope, evidence::Trigger::UserDeletion, Some(auth.user_id)).await?;

    // Replies cascade-delete with their parent (parent_comment_id ON DELETE
    // CASCADE), so comment_count must drop by the whole deleted subtree's
    // size, not just 1 -- minus comments removed by moderation, which no
    // longer count.
    let subtree = sqlx::query_as::<_, (Uuid, bool)>(
        r#"
        WITH RECURSIVE subtree AS (
            SELECT id FROM comments WHERE id = $1
            UNION ALL
            SELECT c.id FROM comments c JOIN subtree s ON c.parent_comment_id = s.id
        )
        SELECT s.id, c.moderation_status != 'removed' FROM subtree s JOIN comments c ON c.id = s.id
        "#
    )
    .bind(comment_id)
    .fetch_all(&mut *tx)
    .await
    .db_err_ctx("Failed to count comment subtree", "Database error")?;
    let deleted_count = subtree.iter().filter(|(_, counted)| *counted).count() as i64;
    let subtree_ids: Vec<Uuid> = subtree.iter().map(|(id, _)| *id).collect();
    // Reports on them that preserved nothing have nothing left to decide.
    let notices = moderation::close_obsolete_reports(&mut tx, "comment", &subtree_ids).await?;

    sqlx::query("DELETE FROM comments WHERE id = $1")
        .bind(comment_id)
        .execute(&mut *tx)
        .await
        .db_err("Failed to delete comment")?;

    sqlx::query("UPDATE posts SET comment_count = GREATEST(comment_count - $1, 0) WHERE id = $2")
        .bind(deleted_count)
        .bind(post_id)
        .execute(&mut *tx)
        .await
        .db_err_ctx("Failed to update comment_count", "Database error")?;

    tx.commit().await.db_err_ctx("Failed to commit transaction", "Database error")?;
    notices.send(&state).await;

    // Comments have no files; this only finishes the bookkeeping.
    evidence::finish(&state, preserved, Vec::new()).await;

    Ok(StatusCode::NO_CONTENT)
}
