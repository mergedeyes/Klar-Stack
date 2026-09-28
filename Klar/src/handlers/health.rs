use axum::{
    extract::{ConnectInfo, Request, State},
    http::StatusCode,
    Json,
};
use std::net::SocketAddr;
use serde::Serialize;

use crate::handlers::auth::AppState;
use crate::rate_limit::{extract_client_ip, trusted_proxy_hops};

#[derive(Serialize)]
pub struct HealthResponse {
    status: String,
    database: String,
}

pub async fn index() -> &'static str {
    "Hallo von Klar!"
}

pub async fn health_check(
    State(state): State<AppState>,
) -> Result<Json<HealthResponse>, StatusCode> {
    let result = sqlx::query("SELECT 1")
        .execute(&state.db)
        .await;

    match result {
        Ok(_) => Ok(Json(HealthResponse {
            status: "ok".to_string(),
            database: "connected".to_string(),
        })),
        Err(_) => Err(StatusCode::SERVICE_UNAVAILABLE),
    }
}

#[derive(Serialize)]
pub struct ClientIpDebugResponse {
    peer_addr: String,
    x_forwarded_for: Vec<String>,
    trusted_proxy_hops: usize,
    resolved_client_ip: String,
}

/// GET /debug/client-ip -- temporary diagnostic for picking the right
/// TRUSTED_PROXY_HOPS (see rate_limit.rs). Shows what this server sees for
/// the caller's own request: the TCP peer, the raw X-Forwarded-For chain,
/// and the IP the rate limiter resolves from it. Open it from two
/// different networks; the setting is right when resolved_client_ip is
/// your real public IP both times.
///
/// Disabled (404) unless DEBUG_CLIENT_IP=true. It only echoes the caller's
/// own request, but it does reveal the proxy chain, so switch it off again
/// once TRUSTED_PROXY_HOPS is confirmed.
pub async fn debug_client_ip(
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    req: Request,
) -> Result<Json<ClientIpDebugResponse>, StatusCode> {
    if std::env::var("DEBUG_CLIENT_IP").as_deref() != Ok("true") {
        return Err(StatusCode::NOT_FOUND);
    }

    let hops = trusted_proxy_hops();

    Ok(Json(ClientIpDebugResponse {
        peer_addr: peer.to_string(),
        x_forwarded_for: req
            .headers()
            .get_all("x-forwarded-for")
            .iter()
            .filter_map(|v| v.to_str().ok().map(str::to_string))
            .collect(),
        trusted_proxy_hops: hops,
        resolved_client_ip: extract_client_ip(&req, hops).to_string(),
    }))
}
