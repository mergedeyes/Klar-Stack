/// Database setup — connection pool creation and migration runner.
/// Everything database-infrastructure lives here.
/// Actual queries live in handlers, close to where they're used.

use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{ConnectOptions, PgPool};
use std::str::FromStr;
use std::time::Duration;

pub async fn create_pool(database_url: &str) -> PgPool {
    let connect_options = PgConnectOptions::from_str(database_url)
        .expect("Invalid DATABASE_URL")
        // Session-level statement_timeout: kills any query that runs longer
        // than this at the Postgres level, guaranteeing a clean error instead
        // of a silent indefinite hang — regardless of the underlying cause
        // (stale pooled connection, Neon cold-start weirdness, a slow query
        // we write by accident later, etc.)
        .options([("statement_timeout", "15000")]);

    PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(10))   // ride out the cold-start wake
        .idle_timeout(Duration::from_secs(180))     // recycle conns before Neon suspends them (~5 min idle)
        .max_lifetime(Duration::from_secs(1500))    // force-recycle every 25 min regardless of activity,
                                                     // so a connection can never silently go stale/zombie
                                                     // against Neon's own connection lifecycle
        .connect_with(connect_options)
        .await
        .expect("Failed to connect to Postgres")
}

pub async fn run_migrations(pool: &PgPool) {
    sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .expect("Failed to run migrations");
}