use axum::{
    extract::{ConnectInfo, Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use governor::{
    clock::{Clock, DefaultClock},
    DefaultKeyedRateLimiter, Quota, RateLimiter,
};
use serde_json::json;
use std::{
    net::{IpAddr, SocketAddr},
    num::NonZeroU32,
    sync::Arc,
    time::Duration,
};

/// How often idle per-IP entries are evicted from the limiter's map.
/// Without this the map only ever grows -- one entry per distinct client
/// IP, forever.
const CLEANUP_INTERVAL: Duration = Duration::from_secs(60);

#[derive(Clone)]
pub struct RateLimitState {
    limiter: Arc<DefaultKeyedRateLimiter<IpAddr>>,
    /// Number of reverse proxies in front of this server that append to
    /// X-Forwarded-For (TRUSTED_PROXY_HOPS). See extract_client_ip.
    trusted_proxy_hops: usize,
}

impl RateLimitState {
    /// Must be called from within the tokio runtime (spawns the cleanup task).
    pub fn new(count: u32, window_secs: u64) -> Self {
        let replenish_interval = Duration::from_secs(window_secs) / count;
        let quota = Quota::with_period(replenish_interval)
            .expect("valid replenish interval")
            .allow_burst(NonZeroU32::new(count).expect("count must be > 0"));

        let limiter = Arc::new(RateLimiter::keyed(quota));

        // retain_recent() drops every key whose bucket has fully refilled,
        // i.e. that behaves exactly like a brand-new entry anyway -- so
        // evicting it changes no rate-limit decision, it only frees memory.
        {
            let limiter = Arc::clone(&limiter);
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(CLEANUP_INTERVAL);
                loop {
                    interval.tick().await;
                    limiter.retain_recent();
                    limiter.shrink_to_fit();
                }
            });
        }

        let trusted_proxy_hops = std::env::var("TRUSTED_PROXY_HOPS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1);

        Self { limiter, trusted_proxy_hops }
    }

    fn check(&self, ip: IpAddr) -> Result<(), u64> {
        match self.limiter.check_key(&ip) {
            Ok(_) => Ok(()),
            Err(not_until) => {
                let wait = not_until.wait_time_from(DefaultClock::default().now());
                Err(wait.as_secs().max(1))
            }
        }
    }
}

/// Resolves the real client IP.
///
/// X-Forwarded-For is a comma-separated list where each proxy *appends*
/// the address it received the request from. Everything to the left of
/// what our own proxies appended was supplied by the client and can be
/// anything -- taking the leftmost entry (as this used to) let anyone
/// bypass the limiter by sending a fresh fake IP with every request.
///
/// So with N trusted proxies (TRUSTED_PROXY_HOPS, default 1 -- the Bunny
/// edge in front of the container), the client is the Nth entry from the
/// right. With 0, the header is ignored and the TCP peer address is used
/// (direct exposure / local dev). If the header has fewer entries than
/// expected, no untrusted value can have been prepended, so the leftmost
/// one is used.
fn extract_client_ip(req: &Request, trusted_proxy_hops: usize) -> IpAddr {
    let peer_ip = req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ci| ci.0.ip())
        .unwrap_or_else(|| IpAddr::from([127, 0, 0, 1]));

    if trusted_proxy_hops == 0 {
        return peer_ip;
    }

    // Multiple X-Forwarded-For headers are equivalent to one
    // comma-joined header, in order.
    let entries: Vec<&str> = req
        .headers()
        .get_all("x-forwarded-for")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(','))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();

    let index = entries.len().saturating_sub(trusted_proxy_hops);

    entries
        .get(index)
        .and_then(|ip| ip.parse::<IpAddr>().ok())
        .unwrap_or(peer_ip)
}

pub async fn rate_limit_middleware(
    State(state): State<RateLimitState>,
    req: Request,
    next: Next,
) -> Response {
    let ip = extract_client_ip(&req, state.trusted_proxy_hops);

    match state.check(ip) {
        Ok(_) => next.run(req).await,
        Err(retry_after) => (
            StatusCode::TOO_MANY_REQUESTS,
            [("retry-after", retry_after.to_string())],
            Json(json!({
                "error": format!("Rate limit exceeded. Try again in {} seconds.", retry_after)
            })),
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;

    fn req(xff: &[&str]) -> Request {
        let mut builder = Request::builder().uri("/");
        for value in xff {
            builder = builder.header("x-forwarded-for", *value);
        }
        let mut req = builder.body(Body::empty()).unwrap();
        req.extensions_mut()
            .insert(ConnectInfo(SocketAddr::from(([10, 0, 0, 1], 1234))));
        req
    }

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn spoofed_leftmost_entry_is_ignored() {
        let r = req(&["6.6.6.6, 1.2.3.4"]);
        assert_eq!(extract_client_ip(&r, 1), ip("1.2.3.4"));
    }

    #[test]
    fn two_hops_takes_second_from_right() {
        let r = req(&["6.6.6.6, 1.2.3.4, 9.9.9.9"]);
        assert_eq!(extract_client_ip(&r, 2), ip("1.2.3.4"));
    }

    #[test]
    fn multiple_headers_are_joined_in_order() {
        let r = req(&["6.6.6.6", "1.2.3.4"]);
        assert_eq!(extract_client_ip(&r, 1), ip("1.2.3.4"));
    }

    #[test]
    fn fewer_entries_than_hops_uses_leftmost() {
        let r = req(&["1.2.3.4"]);
        assert_eq!(extract_client_ip(&r, 2), ip("1.2.3.4"));
    }

    #[test]
    fn zero_hops_ignores_header() {
        let r = req(&["1.2.3.4"]);
        assert_eq!(extract_client_ip(&r, 0), ip("10.0.0.1"));
    }

    #[test]
    fn missing_or_garbage_header_falls_back_to_peer() {
        assert_eq!(extract_client_ip(&req(&[]), 1), ip("10.0.0.1"));
        assert_eq!(extract_client_ip(&req(&["not-an-ip"]), 1), ip("10.0.0.1"));
    }
}
