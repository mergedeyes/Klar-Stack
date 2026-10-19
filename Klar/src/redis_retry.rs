//! Upstash closes idle Redis connections. The ConnectionManager notices
//! only when the next command fails ("broken pipe") and reconnects for the
//! one after, so after a quiet spell the first command of each kind failed:
//! a live notification went missing, or the uptime check reported the
//! backend down for five minutes while it was serving requests fine.

use std::future::Future;

use redis::aio::ConnectionManager;
use redis::RedisResult;

/// Runs a Redis command, and once more if the connection had been dropped.
/// Only then: any other error (a wrong type, a full server) would fail the
/// same way again. A command whose answer was lost on the way back may run
/// twice, which is harmless for the ones used here (PING, PUBLISH, SET of a
/// fresh key; a redeemed ticket is simply not found the second time).
pub async fn retry_once<T, F, Fut>(conn: &ConnectionManager, op: F) -> RedisResult<T>
where
    F: Fn(ConnectionManager) -> Fut,
    Fut: Future<Output = RedisResult<T>>,
{
    match op(conn.clone()).await {
        Err(e) if e.is_connection_dropped() => {
            tracing::warn!("Redis connection was dropped, retrying once: {}", e);
            op(conn.clone()).await
        }
        result => result,
    }
}
