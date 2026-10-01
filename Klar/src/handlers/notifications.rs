use axum::{
    extract::{Query, State},
    response::sse::{Event, Sse},
    Json,
};
use futures::stream::Stream;
use redis::AsyncCommands;
use serde::Serialize;
use std::convert::Infallible;
use tokio::sync::broadcast::error::RecvError;
use uuid::Uuid;

use crate::auth::{generate_refresh_token, hash_refresh_token, AuthUser};
use crate::errors::AppError;
use crate::handlers::auth::AppState;
use crate::models::NotificationActor;
use crate::utils::{DbResultExt, ResolveMedia};

/// Redis pub/sub channel that all backend replicas subscribe to for
/// fanning real-time notifications out across the cluster. See main.rs
/// for the subscriber task that forwards messages on this channel into
/// each replica's local broadcast::channel (which the SSE handler below
/// actually reads from).
pub const NOTIFICATION_CHANNEL: &str = "klar:notifications";

#[derive(Clone, Debug, Serialize, serde::Deserialize)]
pub struct NotificationEvent {
    pub target_user_id: Uuid,
    pub notification: NotificationResponse,
}

#[derive(Debug, Serialize, Clone, serde::Deserialize)]
pub struct NotificationResponse {
    pub id: Uuid,
    pub type_name: String,
    pub is_read: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// None for notices from Klar itself (moderation.rs), which have no
    /// acting user.
    pub actor: Option<NotificationActor>,
    pub post_id: Option<Uuid>,
    /// Storage key (not a full URL) for the post's first image, so the
    /// frontend can show a preview thumbnail on the notification without
    /// a second round-trip. Resolved into a full URL by ResolveMedia
    /// (see utils.rs) before this ever leaves the server, both for the
    /// GET /notifications path and the live SSE push path below --
    /// publish_notification() is the one place both paths funnel through
    /// for the live case. None for notification types with no associated
    /// post (e.g. 'follow').
    pub post_thumb_url: Option<String>,
    /// The statement of reasons a moderation notice refers to.
    pub decision_id: Option<Uuid>,
}

/// Row shape for get_notifications' join, decoded manually via
/// sqlx::query_as with a plain string (not the query!/query_as! macros) —
/// deliberately, so adding post_thumb_url here doesn't require a
/// cargo sqlx prepare run (and a live DB) to update the offline query
/// cache before this builds in CI.
#[derive(sqlx::FromRow)]
struct NotificationRow {
    id: Uuid,
    type_name: Option<String>,
    is_read: bool,
    created_at: chrono::DateTime<chrono::Utc>,
    post_id: Option<Uuid>,
    decision_id: Option<Uuid>,
    actor_id: Option<Uuid>,
    actor_username: Option<String>,
    actor_display: Option<String>,
    actor_avatar: Option<String>,
    post_thumb_url: Option<String>,
}

/// The notification kinds that are persisted in the `notifications` table,
/// mirroring the Postgres `notification_type` enum. An enum rather than
/// string literals at each call site so a typo can't reach the DB cast.
#[derive(Clone, Copy, Debug)]
pub enum NotificationKind {
    Follow,
    FollowRequest,
    FollowAccepted,
    PostLike,
    Comment,
    CommentLike,
}

impl NotificationKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Follow => "follow",
            Self::FollowRequest => "follow_request",
            Self::FollowAccepted => "follow_accepted",
            Self::PostLike => "post_like",
            Self::Comment => "comment",
            Self::CommentLike => "comment_like",
        }
    }
}

/// Store a notification and build the event to push live, for the caller
/// to hand to publish_notification() once its transaction has committed
/// -- publishing before commit could announce something that then rolls
/// back, and publishing inside it would hold the transaction open across
/// a network call.
///
/// Returns None when there's nothing to push: a self-notification (liking
/// your own post), a duplicate (the unique index makes re-liking after an
/// unlike a no-op instead of a second notification), or a failed actor
/// lookup. A failed INSERT is only logged, but note that inside a
/// transaction it still aborts that transaction, so the caller's commit
/// fails as before.
pub async fn insert_notification(
    conn: &mut sqlx::PgConnection,
    target_user_id: Uuid,
    actor_id: Uuid,
    kind: NotificationKind,
    post_id: Option<Uuid>,
    comment_id: Option<Uuid>,
) -> Option<NotificationEvent> {
    if target_user_id == actor_id {
        return None;
    }

    let notification_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO notifications (user_id, actor_id, type, post_id, comment_id)
         VALUES ($1, $2, $3::notification_type, $4, $5)
         ON CONFLICT (user_id, actor_id, type, COALESCE(post_id, '00000000-0000-0000-0000-000000000000'), COALESCE(comment_id, '00000000-0000-0000-0000-000000000000'))
         DO NOTHING RETURNING id"
    )
    .bind(target_user_id)
    .bind(actor_id)
    .bind(kind.as_str())
    .bind(post_id)
    .bind(comment_id)
    .fetch_optional(&mut *conn)
    .await
    .inspect_err(|e| tracing::error!("Failed to insert {} notification: {}", kind.as_str(), e))
    .ok()
    .flatten()?;

    build_event(conn, target_user_id, actor_id, notification_id, kind.as_str(), post_id).await
}

/// Build a live event without storing anything: loads the actor and, for
/// post-related events, the post's first image as a preview thumbnail.
/// Used directly by chats for their live-only "message" signal (those are
/// never written to the notifications table). Best-effort: a post without
/// an image just gets no thumbnail, and a failed actor lookup skips the
/// live push -- the stored row still shows up on the next GET /notifications.
pub async fn build_event(
    conn: &mut sqlx::PgConnection,
    target_user_id: Uuid,
    actor_id: Uuid,
    id: Uuid,
    type_name: &str,
    post_id: Option<Uuid>,
) -> Option<NotificationEvent> {
    let actor_row = sqlx::query_as::<_, crate::models::UserRow>("SELECT * FROM users WHERE id = $1")
        .bind(actor_id)
        .fetch_one(&mut *conn)
        .await
        .ok()?;

    let post_thumb_url = match post_id {
        Some(post_id) => sqlx::query_scalar::<_, String>(
            "SELECT thumb_key FROM media_assets WHERE post_id = $1 AND sort_order = 0"
        )
        .bind(post_id)
        .fetch_optional(&mut *conn)
        .await
        .ok()
        .flatten(),
        None => None,
    };

    Some(NotificationEvent {
        target_user_id,
        notification: NotificationResponse {
            id,
            type_name: type_name.to_string(),
            is_read: false,
            created_at: chrono::Utc::now(),
            post_id,
            post_thumb_url,
            actor: Some(NotificationActor::from(actor_row)),
            decision_id: None,
        },
    })
}

/// Publish a notification event to Redis so every backend replica (not
/// just the one handling this request) can deliver it to any matching SSE
/// subscriber it holds. Errors are logged, not propagated — a failed
/// real-time push shouldn't fail the underlying action (e.g. a like),
/// since the notification row is already durably stored in Postgres and
/// will show up next time the client polls GET /notifications.
///
/// Resolves post_thumb_url/actor.avatar_url into full URLs here, once,
/// before publishing -- every replica's SSE stream just forwards this
/// payload through unchanged (see main.rs's subscriber task and
/// notification_stream below), so this is the only point in the live
/// push path where the active Storage provider is actually available to
/// call. insert_notification()/build_event() still produce raw storage
/// keys, same as everywhere else -- resolution is centralized here, not duplicated
/// at each call site.
pub async fn publish_notification(state: &AppState, event: &NotificationEvent) {
    let resolved_event = NotificationEvent {
        target_user_id: event.target_user_id,
        notification: event.notification.clone().resolve_media(&state.storage),
    };

    let payload = match serde_json::to_string(&resolved_event) {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("Failed to serialize notification event: {}", e);
            return;
        }
    };

    let mut conn = state.redis.clone();
    if let Err(e) = conn.publish::<_, _, ()>(NOTIFICATION_CHANNEL, payload).await {
        tracing::error!("Failed to publish notification to Redis: {}", e);
    }
}

/// GET /notifications — Fetch historical notifications
pub async fn get_notifications(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Vec<NotificationResponse>>, AppError> {

    let records = sqlx::query_as::<_, NotificationRow>(
        r#"
        SELECT 
            n.id, n.type::text as type_name, n.is_read, n.created_at, n.post_id, n.decision_id,
            u.id as actor_id, u.username as actor_username,
            u.display_name as actor_display, u.avatar_url as actor_avatar,
            m.thumb_key as post_thumb_url
        FROM notifications n
        LEFT JOIN users u ON n.actor_id = u.id
        LEFT JOIN media_assets m ON m.post_id = n.post_id AND m.sort_order = 0
        WHERE n.user_id = $1
        ORDER BY n.created_at DESC
        LIMIT 50
        "#
    )
    .bind(auth.user_id)
    .fetch_all(&state.db)
    .await
    .db_err("Database error")?;

    let responses: Vec<NotificationResponse> = records.into_iter().map(|rec| NotificationResponse {
        id: rec.id,
        type_name: rec.type_name.unwrap_or_default(),
        is_read: rec.is_read,
        created_at: rec.created_at,
        post_id: rec.post_id,
        post_thumb_url: rec.post_thumb_url,
        decision_id: rec.decision_id,
        actor: rec.actor_id.zip(rec.actor_username).map(|(id, username)| NotificationActor {
            id,
            username,
            display_name: rec.actor_display,
            avatar_url: rec.actor_avatar,
        }),
    }).collect();

    Ok(Json(responses.resolve_media(&state.storage)))
}

/// How long a stream ticket can be redeemed. Only has to cover the gap
/// between the POST and the EventSource connecting.
const STREAM_TICKET_TTL_SECS: u64 = 30;

fn stream_ticket_key(ticket: &str) -> String {
    format!("klar:sse_ticket:{}", hash_refresh_token(ticket))
}

#[derive(Serialize)]
pub struct StreamTicketResponse {
    pub ticket: String,
}

/// POST /notifications/stream-ticket — a single-use ticket for opening the
/// SSE stream.
///
/// EventSource can't send an Authorization header, so the stream has to
/// be authenticated through the URL. Putting the access token there meant
/// it ended up in the CDN's request logs, still usable for up to 15
/// minutes. A ticket is only valid for STREAM_TICKET_TTL_SECS and is
/// deleted when redeemed, so a logged one is worthless. It lives in Redis
/// (stored hashed, like refresh tokens) so any replica can redeem it.
pub async fn create_stream_ticket(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<StreamTicketResponse>, AppError> {
    let ticket = generate_refresh_token();
    let mut conn = state.redis.clone();
    // With the time it was issued: the stream measures the session against
    // it (account_lock::session_valid).
    let value = format!("{} {}", auth.user_id, crate::auth::issued_now());
    conn.set_ex::<_, _, ()>(stream_ticket_key(&ticket), value, STREAM_TICKET_TTL_SECS)
        .await
        .map_err(|e| {
            tracing::error!("Failed to store SSE ticket: {}", e);
            AppError::internal("Could not create stream ticket")
        })?;

    Ok(Json(StreamTicketResponse { ticket }))
}

/// Redeems a stream ticket: GET and DEL in one transaction, so the same
/// ticket can't open two streams even when both requests race. Returns the
/// user and when the ticket was issued (auth::issued_now).
async fn redeem_stream_ticket(state: &AppState, ticket: &str) -> Result<(Uuid, f64), AppError> {
    let key = stream_ticket_key(ticket);
    let mut conn = state.redis.clone();
    let (user_id, _): (Option<String>, i64) = redis::pipe()
        .atomic()
        .get(&key)
        .del(&key)
        .query_async(&mut conn)
        .await
        .map_err(|e| {
            tracing::error!("Failed to redeem SSE ticket: {}", e);
            AppError::internal("Could not open stream")
        })?;

    user_id
        .and_then(|value| {
            let (user, issued) = value.split_once(' ')?;
            Some((user.parse().ok()?, issued.parse().ok()?))
        })
        .ok_or_else(|| AppError::unauthorized("Invalid or expired stream ticket"))
}

/// How often an open stream checks that its session may go on, so a lock,
/// a new password or the account's deletion closes it soon after.
const STREAM_SESSION_CHECK_SECS: u64 = 30;

async fn stream_session_valid(state: &AppState, user_id: Uuid, since: f64) -> bool {
    match state.db.acquire().await {
        Ok(mut conn) => crate::handlers::account_lock::session_valid(&mut conn, user_id, since).await.unwrap_or(true),
        // The database being briefly unreachable isn't a reason to drop
        // every stream; the next check decides.
        Err(_) => true,
    }
}

#[derive(serde::Deserialize)]
pub struct StreamQuery {
    pub ticket: String,
}

/// GET /notifications/stream?ticket=… — SSE Endpoint
///
/// Reads from the *local* in-process broadcast channel only. Cross-replica
/// delivery happens upstream: publish_notification() PUBLISHes to Redis
/// (already resolved into full URLs at that point), and the subscriber
/// task spawned in main.rs re-broadcasts every message it receives from
/// Redis into this same local channel on every replica — including the
/// replica that originally published it. So this handler doesn't need to
/// know or care about Redis, or about resolving any URLs, at all.
pub async fn notification_stream(
    State(state): State<AppState>,
    Query(query): Query<StreamQuery>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, AppError> {
    let (user_id, issued_at) = redeem_stream_ticket(&state, &query.ticket).await?;
    if !stream_session_valid(&state, user_id, issued_at).await {
        return Err(AppError::unauthorized("Session ended"));
    }
    let mut rx = state.notification_tx.subscribe();
    let mut session_check = tokio::time::interval(std::time::Duration::from_secs(STREAM_SESSION_CHECK_SECS));
    session_check.tick().await;

    let stream = async_stream::stream! {
        loop {
            let received = tokio::select! {
                received = rx.recv() => received,
                _ = session_check.tick() => {
                    if stream_session_valid(&state, user_id, issued_at).await {
                        continue;
                    }
                    // Ended: the client's reconnect needs a new ticket,
                    // which needs a valid session.
                    break;
                }
            };
            let event = match received {
                Ok(event) => event,
                // The channel is shared by every connected client, so a
                // burst of other users' notifications can push this
                // receiver behind. Skipping ahead drops a few events
                // (they are still stored and show up in GET /notifications),
                // whereas ending the loop would silently close the stream.
                Err(RecvError::Lagged(skipped)) => {
                    tracing::warn!("SSE stream for {} lagged, skipped {} events", user_id, skipped);
                    continue;
                }
                Err(RecvError::Closed) => break,
            };
            if event.target_user_id == user_id {
                if let Ok(json) = serde_json::to_string(&event.notification) {
                    yield Ok(Event::default().data(json));
                }
            }
        }
    };

    Ok(Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::new()))
}

/// PATCH /notifications/read — Mark all as read
pub async fn mark_read(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, AppError> {
    sqlx::query!("UPDATE notifications SET is_read = TRUE WHERE user_id = $1", auth.user_id)
        .execute(&state.db)
        .await
        .db_err("Failed to update notifications")?;
        
    Ok(Json(serde_json::json!({"message": "ok"})))
}
