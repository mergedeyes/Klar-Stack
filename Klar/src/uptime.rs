//! Dead-man's-switch for the backend itself, the same scheme the backup
//! sidecar uses: every few minutes each replica checks Postgres and Redis
//! and pings UPTIME_HEALTHCHECK_URL (a healthchecks.io check), or its
//! /fail endpoint when a dependency is down. healthchecks.io alerts on a
//! failure ping at once, and on missing pings once the grace period is
//! over, which covers the server being down entirely.
//!
//! Every replica pings the same check, so it stays green while at least
//! one of them is healthy. The pings carry no body: the privacy policy
//! promises healthchecks.io only content-free success or failure signals.

use std::time::Duration;

use crate::handlers::auth::AppState;

/// Matches the check's period on healthchecks.io (5 minutes, grace 10).
const INTERVAL: Duration = Duration::from_secs(5 * 60);

pub fn spawn(state: AppState) {
    let url = std::env::var("UPTIME_HEALTHCHECK_URL").unwrap_or_default();
    let url = url.trim().trim_end_matches('/').to_string();
    if url.is_empty() {
        tracing::warn!("UPTIME_HEALTHCHECK_URL not set -- nobody is alerted when the backend goes down");
        return;
    }
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .expect("Failed to build HTTP client");

    tokio::spawn(async move {
        loop {
            run_once(&state, &client, &url).await;
            tokio::time::sleep(INTERVAL).await;
        }
    });
}

/// One round: check, then ping success or /fail.
pub(crate) async fn run_once(state: &AppState, client: &reqwest::Client, url: &str) {
    let target = match check(state).await {
        Ok(()) => url.to_string(),
        Err(reason) => {
            tracing::error!("Uptime check failed: {}", reason);
            format!("{url}/fail")
        }
    };
    if let Err(e) = client.get(&target).send().await.and_then(|r| r.error_for_status()) {
        tracing::warn!("Uptime ping failed: {}", e);
    }
}

/// The same dependencies a request needs: the database for everything,
/// Redis for live notifications.
async fn check(state: &AppState) -> Result<(), String> {
    sqlx::query("SELECT 1")
        .execute(&state.db)
        .await
        .map_err(|e| format!("database: {e}"))?;
    let mut redis = state.redis.clone();
    redis::cmd("PING")
        .query_async::<String>(&mut redis)
        .await
        .map_err(|e| format!("redis: {e}"))?;
    Ok(())
}
