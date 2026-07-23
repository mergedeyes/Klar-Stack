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
use futures::StreamExt;
use handlers::auth::AppState;
use crate::storage::Storage;
use tokio::sync::broadcast;

#[tokio::main]
async fn main() {
    eprintln!("=== KLAR BACKEND: main() started ===");
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

    // Redis client — backs the cross-replica notification pub/sub. The
    // `redis::Client` itself is cheap and just holds connection info; the
    // ConnectionManager below is the thing that actually maintains (and
    // auto-reconnects) a connection, and is what gets cloned into AppState
    // for PUBLISHing.
    let redis_client = redis::Client::open(config.redis_url.clone())
        .expect("Invalid REDIS_URL");
    let redis_conn_manager = redis_client
        .get_connection_manager()
        .await
        .expect("Failed to connect to Redis (check REDIS_URL)");
    tracing::info!("Connected to Redis");

    // Local, in-process broadcast channel — every replica has its own.
    // SSE handlers subscribe to this directly. Nothing publishes into it
    // except the Redis subscriber task spawned below, so every replica
    // (including the one that originally handled the triggering request)
    // delivers via the exact same path.
    let (notification_tx, _) = broadcast::channel(100);

    // Background task: SUBSCRIBE to the shared Redis channel forever, and
    // re-broadcast every message received into this replica's local
    // channel. Reconnects with a short backoff if the Redis connection
    // drops, rather than silently going deaf.
    {
        let redis_client = redis_client.clone();
        let notification_tx = notification_tx.clone();
        tokio::spawn(async move {
            loop {
                match redis_client.get_async_pubsub().await {
                    Ok(mut pubsub) => {
                        if let Err(e) = pubsub
                            .subscribe(handlers::notifications::NOTIFICATION_CHANNEL)
                            .await
                        {
                            tracing::error!("Failed to subscribe to Redis notifications channel: {}", e);
                            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                            continue;
                        }
                        tracing::info!("Subscribed to Redis channel '{}'", handlers::notifications::NOTIFICATION_CHANNEL);

                        let mut stream = pubsub.on_message();
                        while let Some(msg) = stream.next().await {
                            let payload: String = match msg.get_payload() {
                                Ok(p) => p,
                                Err(e) => {
                                    tracing::error!("Bad Redis pub/sub payload: {}", e);
                                    continue;
                                }
                            };
                            match serde_json::from_str::<handlers::notifications::NotificationEvent>(&payload) {
                                Ok(event) => {
                                    // No subscribers on this replica right now is
                                    // fine and common — send() erroring just means
                                    // that, not a real failure.
                                    let _ = notification_tx.send(event);
                                }
                                Err(e) => tracing::error!("Failed to deserialize notification event: {}", e),
                            }
                        }

                        tracing::warn!("Redis pub/sub stream ended unexpectedly, reconnecting...");
                    }
                    Err(e) => {
                        tracing::error!("Failed to open Redis pub/sub connection: {}", e);
                    }
                }
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
        });
    }

    let state = AppState {
        db: pool,
        jwt_secret: config.jwt_secret,
        storage,
        email,
        notification_tx,
        redis: redis_conn_manager,
    };

    let app = routes::create_router(state);
    tracing::info!("Server running on http://{}", addr);

    eprintln!("=== KLAR BACKEND: about to bind {} ===", addr);
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
