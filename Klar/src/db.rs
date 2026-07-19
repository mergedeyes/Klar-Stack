/// Database setup — connection pool creation and migration runner.
/// Everything database-infrastructure lives here.
/// Actual queries live in handlers, close to where they're used.

use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::time::Duration;

pub async fn create_pool(database_url: &str) -> PgPool {
    PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(10))   // ride out the cold-start wake
        .idle_timeout(Duration::from_secs(180))     // recycle conns before Neon suspends them (~5 min idle)
        .connect(database_url)
        .await
        .expect("Failed to connect to Postgres")
}

pub async fn run_migrations(pool: &PgPool) {
    sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .expect("Failed to run migrations");
}