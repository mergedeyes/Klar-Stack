/// Shared error response type.
/// Every handler that can fail returns this shape,
/// so the API is consistent for clients.

use axum::{
    http::StatusCode,
    Json,
    response::IntoResponse,
};
use serde::Serialize;

#[derive(Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

// This is an Axum trick: by implementing IntoResponse for our own error type,
// we can use it directly as a return type in handlers with the ? operator later.
// For now we're still using Result tuples, but this sets us up for cleaner error handling.
#[derive(Debug)]
pub struct AppError {
    pub status: StatusCode,
    pub message: String,
}

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        // Centralized logging: every error response gets logged here, so
        // individual handlers don't each need to remember to call
        // tracing::error!/warn! before returning an AppError. 5xx (our bugs
        // or infra problems) log at ERROR; 4xx (expected client-side
        // rejections like bad input or permission checks) log at WARN so
        // they're still visible in the console without being alarm-level.
        if self.status.is_server_error() {
            tracing::error!(status = %self.status, "{}", self.message);
        } else {
            tracing::warn!(status = %self.status, "{}", self.message);
        }

        let body = Json(ErrorResponse {
            error: self.message,
        });
        (self.status, body).into_response()
    }
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        AppError::internal(format!("Database error: {}", err))
    }
}

impl AppError {
    pub fn not_found(msg: impl Into<String>) -> Self {
        Self { status: StatusCode::NOT_FOUND, message: msg.into() }
    }

    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self { status: StatusCode::BAD_REQUEST, message: msg.into() }
    }

    pub fn conflict(msg: impl Into<String>) -> Self {
        Self { status: StatusCode::CONFLICT, message: msg.into() }
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        Self { status: StatusCode::INTERNAL_SERVER_ERROR, message: msg.into() }
    }

    pub fn unauthorized(msg: impl Into<String>) -> Self {
        Self {
            status: axum::http::StatusCode::UNAUTHORIZED,
            message: msg.into(),
        }
    }

    pub fn forbidden(msg: impl Into<String>) -> Self {
        Self {
            status: axum::http::StatusCode::FORBIDDEN,
            message: msg.into(),
        }
    }
}