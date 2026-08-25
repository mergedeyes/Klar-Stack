/// Database setup: connection pool creation and migration runner.
/// Everything database-infrastructure lives here.
/// Actual queries live in handlers, close to where they're used.

use sqlx::postgres::{PgConnectOptions, PgPoolOptions}; // typed builders for a single Postgres connection and for the pool around it
use sqlx::PgPool; // the connection pool type handed out to the rest of the app
use std::str::FromStr; // brings the from_str constructor into scope for PgConnectOptions
use std::time::Duration; // used for all the timeout/lifetime settings below

pub async fn create_pool(database_url: &str) -> PgPool {
    // Parse the DATABASE_URL into structured connection options (host, port, user, etc).
    // Panics on startup if the URL is malformed, since there is no reasonable way to run without it.
    let connect_options = PgConnectOptions::from_str(database_url)
        .expect("Invalid DATABASE_URL")
        // Session-level statement_timeout: kills any query that runs longer
        // than this at the Postgres level, guaranteeing a clean error instead
        // of a silent indefinite hang, whatever the cause.
        .options([("statement_timeout", "15000")]); // 15000 ms = 15 seconds

    let pool_options = PgPoolOptions::new()
        .max_connections(10) // upper limit on simultaneously open connections in the pool
        // Postgres now runs as a container in the same pod (reached over
        // localhost): no cold-start, no idle-suspend. The old Neon-specific
        // acquire/idle/lifecycle tuning is therefore gone; we only force-recycle
        // every 30 min as a cheap guard against a connection going stale.
        .acquire_timeout(Duration::from_secs(5)) // how long to wait for a free connection before giving up
        .max_lifetime(Duration::from_secs(1800)); // 1800 s = 30 min; a connection older than this gets closed and replaced

    // On pod startup the backend and Postgres come up together, and on the
    // very first boot the DB container additionally needs time for initdb.
    // Rather than panicking on the initial connect, back off and retry until
    // Postgres is accepting connections.
    let mut attempt = 0;
    loop {
        attempt += 1;
        match pool_options
            .clone() // PgPoolOptions/PgConnectOptions are consumed by connect_with, so both
            .connect_with(connect_options.clone()) // are cloned here to allow retrying the loop with the same settings
            .await
        {
            Ok(pool) => return pool, // connected successfully, hand the pool back to the caller
            Err(e) if attempt < 30 => {
                // Not yet at the retry limit: log a warning and wait 2 seconds before trying again.
                tracing::warn!(
                    "Postgres not ready yet (attempt {attempt}): {e}. Retrying in 2s…"
                );
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
            // Exhausted all 30 attempts (roughly a minute of retrying): something is
            // genuinely wrong, so give up and crash loudly instead of retrying forever.
            Err(e) => panic!("Failed to connect to Postgres after {attempt} attempts: {e}"),
        }
    }
}

pub async fn run_migrations(pool: &PgPool) {
    // sqlx::migrate! embeds the SQL files from ./migrations into the compiled binary
    // at build time (so no separate migrations folder needs to ship alongside it),
    // then .run applies any migrations not yet recorded as done in this database.
    // Panics if a migration fails, since starting the app against a half-migrated
    // schema would be worse than failing fast here.
    sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .expect("Failed to run migrations");
}
