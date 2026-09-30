//! Test-phase opt-in: all data is wiped before launch, except accounts
//! whose owners asked to keep theirs. The choice is the user's alone and
//! can be changed until the wipe; `keep_after_test_at` records when they
//! opted in (for the wipe and as a record of the request). Remove this
//! module and its column once Klar has launched.

use axum::{extract::State, Json};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::auth::AuthUser;
use crate::errors::AppError;
use crate::handlers::auth::AppState;
use crate::utils::DbResultExt;

#[derive(Debug, Serialize)]
pub struct KeepAccountResponse {
    pub keep: bool,
    pub since: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
pub struct KeepAccountRequest {
    pub keep: bool,
}

/// GET /users/me/keep-account
pub async fn get_keep_account(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<KeepAccountResponse>, AppError> {
    let since = sqlx::query_scalar::<_, Option<DateTime<Utc>>>("SELECT keep_after_test_at FROM users WHERE id = $1")
        .bind(auth.user_id)
        .fetch_one(&state.db)
        .await
        .db_err("Database error")?;
    Ok(Json(KeepAccountResponse { keep: since.is_some(), since }))
}

/// PATCH /users/me/keep-account -- opting in again keeps the first date.
pub async fn set_keep_account(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<KeepAccountRequest>,
) -> Result<Json<KeepAccountResponse>, AppError> {
    let since = sqlx::query_scalar::<_, Option<DateTime<Utc>>>(
        r#"
        UPDATE users SET keep_after_test_at = CASE WHEN $2 THEN COALESCE(keep_after_test_at, NOW()) END
        WHERE id = $1
        RETURNING keep_after_test_at
        "#,
    )
    .bind(auth.user_id)
    .bind(input.keep)
    .fetch_one(&state.db)
    .await
    .db_err("Database error")?;
    tracing::info!("User {} {} keeping their account after the test", auth.user_id, if input.keep { "opted in to" } else { "opted out of" });
    Ok(Json(KeepAccountResponse { keep: since.is_some(), since }))
}
