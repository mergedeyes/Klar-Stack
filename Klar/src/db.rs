/// Database setup — connection pool creation and migration runner.
/// Everything database-infrastructure lives here.
/// Actual queries live in handlers, close to where they're used.

use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::PgPool;
use std::str::FromStr;
use std::time::Duration;

pub async fn create_pool(database_url: &str) -> PgPool {
    let connect_options = PgConnectOptions::from_str(database_url)
        .expect("Invalid DATABASE_URL")
        // Session-level statement_timeout: kills any query that runs longer
        // than this at the Postgres level, guaranteeing a clean error instead
        // of a silent indefinite hang, whatever the cause.
        .options([("statement_timeout", "15000")]);

    let pool_options = PgPoolOptions::new()
        .max_connections(10)
        // Postgres now runs as a container in the same pod (reached over
        // localhost): no cold-start, no idle-suspend. The old Neon-specific
        // acquire/idle/lifecycle tuning is therefore gone; we only force-recycle
        // every 30 min as a cheap guard against a connection going stale.
        .acquire_timeout(Duration::from_secs(5))
        .max_lifetime(Duration::from_secs(1800));

    // On pod startup the backend and Postgres come up together, and on the
    // very first boot the DB container additionally needs time for initdb.
    // Rather than panicking on the initial connect, back off and retry until
    // Postgres is accepting connections.
    let mut attempt = 0;
    loop {
        attempt += 1;
        match pool_options
            .clone()
            .connect_with(connect_options.clone())
            .await
        {
            Ok(pool) => return pool,
            Err(e) if attempt < 30 => {
                tracing::warn!(
                    "Postgres not ready yet (attempt {attempt}): {e}. Retrying in 2s…"
                );
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
            Err(e) => panic!("Failed to connect to Postgres after {attempt} attempts: {e}"),
        }
    }
}

pub async fn run_migrations(pool: &PgPool) {
    sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .expect("Failed to run migrations");
}
