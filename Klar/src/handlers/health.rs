use axum::{
    extract::State,
    http::StatusCode,
    Json,
};
use serde::Serialize;

use crate::handlers::auth::AppState;

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