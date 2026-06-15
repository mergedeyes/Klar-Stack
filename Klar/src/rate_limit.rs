use axum::{
    extract::{ConnectInfo, Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use governor::{
    clock::{Clock, DefaultClock},
    Quota, RateLimiter,
};
use serde_json::json;
use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    num::NonZeroU32,
    sync::{Arc, Mutex},
    time::Duration,
};

type DirectLimiter = RateLimiter<
    governor::state::NotKeyed,
    governor::state::InMemoryState,
    DefaultClock,
>;

#[derive(Clone)]
pub struct RateLimitState {
    limiters: Arc<Mutex<HashMap<IpAddr, Arc<DirectLimiter>>>>,
    quota: Quota,
}

impl RateLimitState {
    pub fn new(count: u32, window_secs: u64) -> Self {
        let replenish_interval = Duration::from_secs(window_secs) / count;
        let quota = Quota::with_period(replenish_interval)
            .expect("valid replenish interval")
            .allow_burst(NonZeroU32::new(count).expect("count must be > 0"));

        Self {
            limiters: Arc::new(Mutex::new(HashMap::new())),
            quota,
        }
    }

    fn check(&self, ip: IpAddr) -> Result<(), u64> {
        let limiter = {
            let mut map = self.limiters.lock().unwrap();
            map.entry(ip)
                .or_insert_with(|| Arc::new(RateLimiter::direct(self.quota)))
                .clone()
        };

        match limiter.check() {
            Ok(_) => Ok(()),
            Err(not_until) => {
                let clock = DefaultClock::default();
                let wait = not_until.wait_time_from(clock.now());
                Err(wait.as_secs().max(1))
            }
        }
    }
}

fn extract_client_ip(req: &Request) -> IpAddr {
    if let Some(forwarded) = req.headers().get("x-forwarded-for") {
        if let Ok(value) = forwarded.to_str() {
            if let Some(first) = value.split(',').next() {
                if let Ok(ip) = first.trim().parse::<IpAddr>() {
                    return ip;
                }
            }
        }
    }

    req.extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ci| ci.0.ip())
        .unwrap_or_else(|| IpAddr::from([127, 0, 0, 1]))
}

pub async fn rate_limit_middleware(
    State(state): State<RateLimitState>,
    req: Request,
    next: Next,
) -> Response {
    let ip = extract_client_ip(&req);

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