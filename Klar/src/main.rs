mod auth;
mod config;
mod db;
mod email;
mod errors;
mod handlers;
mod media;
mod models;
mod rate_limit;
mod routes;
mod storage;

use email::{EmailProvider, EmailService};
use handlers::auth::AppState;
use crate::storage::Storage;
use tokio::sync::broadcast;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    dotenvy::dotenv().ok();

    let config = config::Config::from_env();

    let pool = db::create_pool(&config.database_url).await;
    tracing::info!("Connected to database");

    db::run_migrations(&pool).await;
    tracing::info!("Migrations complete");

    // Initialize Storage
    let storage = Storage::new().await;
    tracing::info!("Cloud storage client connected");

    // Email service
    let provider: EmailProvider = std::env::var("EMAIL_PROVIDER")
        .expect("EMAIL_PROVIDER missing")
        .parse()
        .expect("Invalid EMAIL_PROVIDER");

    let email = EmailService::new(
        provider,
        &config.smtp_host,
        config.smtp_port,
        &config.smtp_from,
        config.smtp_pass.as_deref(),
        &config.base_url,
    );
    tracing::info!("Email service ready (SMTP: {}:{})", config.smtp_host, config.smtp_port);

    let addr = config.addr();

    // Broadcast channel for real-time events (like notifications)
    let (notification_tx, _) = broadcast::channel(100);

    let state = AppState {
        db: pool,
        jwt_secret: config.jwt_secret,
        storage,
        email,
        notification_tx,
    };

    let app = routes::create_router(state);
    tracing::info!("Server running on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("Failed to bind to address");

    // into_make_service_with_connect_info gives middleware access to the
    // client's socket address (used by rate_limit for per-IP tracking)
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await
    .expect("Server failed");
}