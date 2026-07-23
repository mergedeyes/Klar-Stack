use axum::{extract::{State, Path}, http::StatusCode, Json};
use uuid::Uuid;
use chrono::Utc;
use crate::{AppState, errors::AppError, auth::AuthUser, models::chat::*};
use crate::handlers::notifications::{publish_notification, NotificationEvent, NotificationResponse};

pub async fn get_conversations(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Vec<ConversationResponse>>, AppError> {
    // Converted from the query_as! macro to a plain sqlx::query_as call
    // (runtime-checked, not compile-time) so adding last_message_sender_id
    // here doesn't require a cargo sqlx prepare run against a live DB to
    // refresh the offline query cache before this builds in CI.
    let convos = sqlx::query_as::<_, ConversationResponse>(
        r#"
        SELECT 
            c.id,
            u.id as other_user_id,
            u.username as other_username,
            u.avatar_url as other_avatar_url,
            lm.body as last_message,
            lm.sender_id as last_message_sender_id,
            c.updated_at
        FROM conversations c
        JOIN users u ON u.id = CASE WHEN c.user1_id = $1 THEN c.user2_id ELSE c.user1_id END
        LEFT JOIN LATERAL (
            SELECT body, sender_id FROM messages m
            WHERE m.conversation_id = c.id
            ORDER BY m.created_at DESC
            LIMIT 1
        ) lm ON true
        WHERE c.user1_id = $1 OR c.user2_id = $1
        ORDER BY c.updated_at DESC
        "#
    )
    .bind(auth.user_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| AppError::internal(&format!("DB Error: {}", e)))?;

    Ok(Json(convos))
}

pub async fn get_messages(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(conversation_id): Path<Uuid>,
) -> Result<Json<Vec<MessageResponse>>, AppError> {
    let has_access = sqlx::query!(
        "SELECT 1 as access FROM conversations WHERE id = $1 AND (user1_id = $2 OR user2_id = $2)",
        conversation_id, auth.user_id
    )
    .fetch_optional(&state.db)
    .await?;

    if has_access.is_none() {
        return Err(AppError::forbidden("Access denied to this conversation"));
    }

    let messages = sqlx::query_as::<_, MessageResponse>(
        r#"
        SELECT 
            m.id, m.conversation_id, m.sender_id, m.body, m.created_at, m.edited_at, m.is_read, m.reply_to_message_id,
            COALESCE(
                (
                    SELECT json_agg(json_build_object('emoji', mr.emoji, 'user_id', mr.user_id, 'username', u.username))
                    FROM message_reactions mr
                    JOIN users u ON u.id = mr.user_id
                    WHERE mr.message_id = m.id
                ), 
                '[]'::json
            ) as reactions
        FROM messages m
        WHERE m.conversation_id = $1
        ORDER BY m.created_at ASC
        "#
    )
    .bind(conversation_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| AppError::internal(&format!("DB Error: {}", e)))?;

    Ok(Json(messages))
}

pub async fn send_message(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(payload): Json<SendMessageRequest>,
) -> Result<Json<MessageResponse>, AppError> {
    if auth.user_id == payload.receiver_id {
        return Err(AppError::bad_request("You cannot message yourself"));
    }

    let mutual_follow_count = sqlx::query!(
        r#"
        SELECT COUNT(*) as count 
        FROM follows 
        WHERE (follower_id = $1 AND following_id = $2)
           OR (follower_id = $2 AND following_id = $1)
        "#,
        auth.user_id, payload.receiver_id
    )
    .fetch_one(&state.db)
    .await?
    .count;

    if mutual_follow_count != Some(2) {
        return Err(AppError::forbidden("You can only message users who follow you back."));
    }

    let conv_record = sqlx::query!(
        r#"
        INSERT INTO conversations (user1_id, user2_id)
        VALUES (least($1::uuid, $2::uuid), greatest($1::uuid, $2::uuid))
        ON CONFLICT (least(user1_id, user2_id), greatest(user1_id, user2_id))
        DO UPDATE SET updated_at = NOW()
        RETURNING id
        "#,
        auth.user_id, payload.receiver_id
    )
    .fetch_one(&state.db)
    .await?;

    let message_id = sqlx::query!(
        r#"
        INSERT INTO messages (conversation_id, sender_id, body, reply_to_message_id)
        VALUES ($1, $2, $3, $4)
        RETURNING id
        "#,
        conv_record.id, auth.user_id, payload.body, payload.reply_to_message_id
    )
    .fetch_one(&state.db)
    .await?
    .id;

    let message = sqlx::query_as::<_, MessageResponse>(
        r#"
        SELECT id, conversation_id, sender_id, body, created_at, edited_at, is_read, reply_to_message_id, '[]'::json as reactions
        FROM messages WHERE id = $1
        "#
    )
    .bind(message_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| AppError::internal(&format!("DB Error: {}", e)))?;

    // Piggyback on the same Redis-backed SSE pipeline used for
    // notifications, rather than standing up a second channel/stream just
    // for this. type_name "message" is never written to the `notifications`
    // table (chat messages aren't persisted there) -- this is purely a
    // live signal for the chat icon's unread badge; the frontend's SSE
    // listener special-cases this type_name instead of adding it to the
    // notification dropdown list. `id` here is the message's own id, not
    // a notifications-table row id, since none exists.
    if let Ok(actor_row) = sqlx::query_as::<_, crate::models::UserRow>("SELECT * FROM users WHERE id = $1")
        .bind(auth.user_id)
        .fetch_one(&state.db)
        .await
    {
        let event = NotificationEvent {
            target_user_id: payload.receiver_id,
            notification: NotificationResponse {
                id: message_id,
                type_name: "message".to_string(),
                is_read: false,
                created_at: Utc::now(),
                post_id: None,
                // No post involved in a chat message.
                post_thumb_url: None,
                actor: crate::models::UserResponse::from(actor_row),
            }
        };
        publish_notification(&state, &event).await;
    }

    Ok(Json(message))
}

pub async fn edit_message(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(message_id): Path<Uuid>,
    Json(payload): Json<EditMessageRequest>,
) -> Result<StatusCode, AppError> {
    let msg_meta = sqlx::query!("SELECT sender_id FROM messages WHERE id = $1", message_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::not_found("Message not found"))?;

    if msg_meta.sender_id != auth.user_id {
        return Err(AppError::forbidden("You can only edit your own messages"));
    }

    sqlx::query!(
        "UPDATE messages SET body = $1, edited_at = $2 WHERE id = $3",
        payload.body, Utc::now(), message_id
    )
    .execute(&state.db)
    .await?;

    Ok(StatusCode::OK)
}

pub async fn delete_message(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(message_id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let msg_meta = sqlx::query!("SELECT sender_id FROM messages WHERE id = $1", message_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::not_found("Message not found"))?;

    if msg_meta.sender_id != auth.user_id {
        return Err(AppError::forbidden("You can only delete your own messages"));
    }

    sqlx::query!("DELETE FROM messages WHERE id = $1", message_id)
        .execute(&state.db)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn toggle_reaction(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(message_id): Path<Uuid>,
    Json(payload): Json<ToggleReactionRequest>,
) -> Result<StatusCode, AppError> {
    let existing = sqlx::query!(
        "SELECT 1 as has_reacted FROM message_reactions WHERE message_id = $1 AND user_id = $2 AND emoji = $3",
        message_id, auth.user_id, payload.emoji
    )
    .fetch_optional(&state.db)
    .await?;

    if existing.is_some() {
        sqlx::query!(
            "DELETE FROM message_reactions WHERE message_id = $1 AND user_id = $2 AND emoji = $3",
            message_id, auth.user_id, payload.emoji
        )
        .execute(&state.db)
        .await?;
    } else {
        sqlx::query!(
            "INSERT INTO message_reactions (message_id, user_id, emoji) VALUES ($1, $2, $3)",
            message_id, auth.user_id, payload.emoji
        )
        .execute(&state.db)
        .await?;
    }

    // Notify the *other* participant in this conversation -- reusing the
    // same "message" SSE event type as send_message, rather than adding a
    // separate "reaction" type, since the effect wanted is identical: bump
    // the Chat icon's badge, and make an open ChatWindow on the other end
    // live-refetch to show the new/removed reaction. A 1:1 conversation
    // always has exactly two participants, so "whichever of user1/user2
    // isn't me" is always the right target regardless of who sent the
    // message being reacted to.
    if let Ok(Some((user1_id, user2_id))) = sqlx::query_as::<_, (Uuid, Uuid)>(
        r#"
        SELECT c.user1_id, c.user2_id
        FROM conversations c
        JOIN messages m ON m.conversation_id = c.id
        WHERE m.id = $1
        "#
    )
    .bind(message_id)
    .fetch_optional(&state.db)
    .await
    {
        let target_user_id = if user1_id == auth.user_id { user2_id } else { user1_id };

        if let Ok(actor_row) = sqlx::query_as::<_, crate::models::UserRow>("SELECT * FROM users WHERE id = $1")
            .bind(auth.user_id)
            .fetch_one(&state.db)
            .await
        {
            let event = NotificationEvent {
                target_user_id,
                notification: NotificationResponse {
                    id: message_id,
                    type_name: "message".to_string(),
                    is_read: false,
                    created_at: Utc::now(),
                    post_id: None,
                    post_thumb_url: None,
                    actor: crate::models::UserResponse::from(actor_row),
                }
            };
            publish_notification(&state, &event).await;
        }
    }

    Ok(StatusCode::OK)
}

/// PATCH /chats/:conversation_id/read — mark every message in this
/// conversation that isn't the caller's own as read. Called by the
/// frontend when a conversation is opened, so the unread badge clears.
pub async fn mark_conversation_read(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(conversation_id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let has_access = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM conversations WHERE id = $1 AND (user1_id = $2 OR user2_id = $2))"
    )
    .bind(conversation_id)
    .bind(auth.user_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| AppError::internal(&format!("DB Error: {}", e)))?;

    if !has_access {
        return Err(AppError::forbidden("Access denied to this conversation"));
    }

    sqlx::query(
        "UPDATE messages SET is_read = TRUE WHERE conversation_id = $1 AND sender_id != $2 AND is_read = FALSE"
    )
    .bind(conversation_id)
    .bind(auth.user_id)
    .execute(&state.db)
    .await
    .map_err(|e| AppError::internal(&format!("DB Error: {}", e)))?;

    Ok(StatusCode::NO_CONTENT)
}

/// GET /chats/unread-count — total unread messages across all of the
/// caller's conversations, for the Chat icon's red-dot badge.
pub async fn get_unread_count(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<UnreadCountResponse>, AppError> {
    let count = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*)
        FROM messages m
        JOIN conversations c ON c.id = m.conversation_id
        WHERE (c.user1_id = $1 OR c.user2_id = $1)
          AND m.sender_id != $1
          AND m.is_read = FALSE
        "#
    )
    .bind(auth.user_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| AppError::internal(&format!("DB Error: {}", e)))?;

    Ok(Json(UnreadCountResponse { count }))
}
