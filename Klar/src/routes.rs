use axum::{
    http::{HeaderValue, Method},
    middleware,
    routing::{get, patch, post},
    Router,
};
use tower_http::cors::{AllowHeaders, CorsLayer};
use tower_http::trace::{DefaultMakeSpan, DefaultOnFailure, DefaultOnResponse, TraceLayer};
use tracing::Level;

use crate::handlers;
use crate::handlers::auth::AppState;
use crate::rate_limit::{self, RateLimitState};
use axum::extract::DefaultBodyLimit;

/// Build the CORS layer from the CORS_ORIGINS env var.
fn build_cors() -> CorsLayer {
    let origins: Vec<HeaderValue> = std::env::var("CORS_ORIGINS")
        .unwrap_or_else(|_| "http://localhost:5173".to_string())
        .split(',')
        .map(|s| s.trim().to_string())
        .filter_map(|origin| origin.parse::<HeaderValue>().ok())
        .collect();

    CorsLayer::new()
        .allow_origin(origins)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers(AllowHeaders::mirror_request())
        .allow_credentials(true)
}

/// Build the request/response tracing layer. Logs every request — method,
/// path, status code, and latency — at INFO, so failures are always visible
/// in the server console even for handlers that don't call tracing:: themselves.
fn build_trace_layer() -> TraceLayer<tower_http::classify::SharedClassifier<tower_http::classify::ServerErrorsAsFailures>> {
    TraceLayer::new_for_http()
        .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
        .on_response(DefaultOnResponse::new().level(Level::INFO).latency_unit(tower_http::LatencyUnit::Millis))
        .on_failure(DefaultOnFailure::new().level(Level::ERROR))
}

pub fn create_router(state: AppState) -> Router {
    // ── Rate limiters ───────────────────────────────────────────
    // Auth limit is configurable via AUTH_RATE_LIMIT_PER_MIN (defaults to the
    // production value of 15) so local/test runs that register many users
    // in quick succession don't need to weaken the real production limit.
    let auth_rate_limit: u32 = std::env::var("AUTH_RATE_LIMIT_PER_MIN")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(15);
    let auth_limiter = RateLimitState::new(auth_rate_limit, 60);
    let general_limiter = RateLimitState::new(500, 60);

    // ── Auth routes (strict rate limit) ─────────────────────────
    let auth_routes = Router::new()
        .route("/auth/register", post(handlers::auth::register))
        .route("/auth/login", post(handlers::auth::login))
        .route("/auth/refresh", post(handlers::auth::refresh))
        .route("/auth/logout", post(handlers::auth::logout))
        .route("/auth/verify", get(handlers::auth::verify_email))
        .route("/auth/forgot-password", post(handlers::auth::forgot_password))
        .route("/auth/reset-password", post(handlers::auth::reset_password))
        .route("/auth/resend-verification", post(handlers::auth::resend_verification))
        .route_layer(middleware::from_fn_with_state(
            auth_limiter,
            rate_limit::rate_limit_middleware,
        ));

    // ── API routes (general rate limit) ─────────────────────────
    let api_routes = Router::new()
        // Public
        .route("/", get(handlers::health::index))
        .route("/health", get(handlers::health::health_check))

        // User search (must be before /users/{username})
        .route("/users/search", get(handlers::users::search_users))

        // Users (public)
        .route("/users/{username}", get(handlers::users::get_user))
        .route("/users/{username}/posts", get(handlers::posts::get_user_posts))
        .route("/users/{username}/stats", get(handlers::follows::get_user_stats))
        .route("/users/{username}/followers", get(handlers::follows::get_followers))
        .route("/users/{username}/following", get(handlers::follows::get_following))

        // Users (auth required)
        .route("/users/me", get(handlers::users::get_me)
            .patch(handlers::users::update_profile)
            .delete(handlers::users::delete_account))
        .route("/users/me/password", patch(handlers::users::change_password))
        .route("/users/me/avatar", post(handlers::users::upload_avatar))
        .route("/users/me/blocked", get(handlers::blocks::get_blocked_users))
        .route("/users/me/export", get(handlers::users::export_my_data))
        .route("/users/{username}/follow", post(handlers::follows::follow_user)
            .delete(handlers::follows::unfollow_user))
        .route("/users/{username}/block", post(handlers::blocks::block_user)
            .delete(handlers::blocks::unblock_user))

        // Posts
        .route("/posts", post(handlers::posts::create_post))
        .route("/posts/upload", post(handlers::uploads::upload_post)
            .layer(DefaultBodyLimit::max(20 * 1024 * 1024)) // <-- Das hier hinzufügen
        )
        .route("/posts/{post_id}", get(handlers::posts::get_post)
            .patch(handlers::posts::edit_post)
            .delete(handlers::posts::delete_post))
        .route("/posts/{post_id}/media", get(handlers::uploads::get_post_media))

        // Likes
        .route("/posts/{post_id}/like", post(handlers::likes::toggle_like))
        .route("/posts/{post_id}/likes", get(handlers::likes::get_likes))

        // Comments
        .route("/posts/{post_id}/comments", post(handlers::comments::create_comment)
            .get(handlers::comments::get_comments))
        .route("/posts/{post_id}/comments/{comment_id}",
            patch(handlers::comments::edit_comment)
            .delete(handlers::comments::delete_comment))
        .route("/posts/{post_id}/comments/{comment_id}/like",
            post(handlers::comment_likes::toggle_comment_like))

        // ── Chats ──────────────────────────────────────────────
        .route("/chats", get(handlers::chats::get_conversations))
        .route("/chats/send", post(handlers::chats::send_message))
        .route("/chats/{conversation_id}/messages", get(handlers::chats::get_messages))
        .route("/chats/messages/{message_id}", 
            patch(handlers::chats::edit_message)
            .delete(handlers::chats::delete_message))
        .route("/chats/messages/{message_id}/reactions", post(handlers::chats::toggle_reaction))
        // ────────────────────────────────────────────────────────────

        // Feed (auth required)
        .route("/feed", get(handlers::posts::get_feed))
        .route("/feed/discovery", get(handlers::feed::get_global_feed))

        // Notifications
        .route("/notifications", get(handlers::notifications::get_notifications))
        .route("/notifications/stream", get(handlers::notifications::notification_stream))
        .route("/notifications/read", patch(handlers::notifications::mark_read))

        // Interaction event log (client-reported views)
        .route("/events", post(handlers::events::create_event))

        .route_layer(middleware::from_fn_with_state(
            general_limiter,
            rate_limit::rate_limit_middleware,
        ));

    // ── Combine ─────────────────────────────────────────────────
    Router::new()
        .merge(auth_routes)
        .merge(api_routes)
        .layer(build_cors())
        .layer(build_trace_layer())
        .with_state(state)
}