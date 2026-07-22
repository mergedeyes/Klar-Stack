/// Auth handlers — registration, login, refresh, logout, email verification, password reset.

use axum::{
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    Json,
};
use argon2::{
    password_hash::{rand_core::OsRng, SaltString, PasswordHasher, PasswordHash, PasswordVerifier},
    Argon2,
};
use rand::Rng;
use serde::Deserialize;

use crate::auth::{create_access_token, generate_refresh_token, hash_refresh_token};
use crate::email::EmailService;
use crate::errors::AppError;
use crate::models::{
    AuthResponse, LoginRequest, RefreshResponse,
    RegisterRequest, UserResponse, UserRow,
};
use crate::storage::Storage;

#[derive(Clone)]
pub struct AppState {
    pub db: sqlx::PgPool,
    pub jwt_secret: String,
    pub storage: Storage,
    pub email: EmailService,
    pub notification_tx: tokio::sync::broadcast::Sender<crate::handlers::notifications::NotificationEvent>,
}

/// Helper to generate secure Set-Cookie headers
fn build_auth_cookies(access: &str, refresh: &str) -> HeaderMap {
    // Prüfen, ob wir in Produktion sind (z.B. über eine ENV-Variable)
    let is_prod = std::env::var("ENV").unwrap_or_default() == "production";

    // Lokal lassen wir "Secure" weg, online erzwingen wir es.
    let secure_flag = if is_prod { "Secure; " } else { "" };

    // klarsocial.eu and klarsocial.de are genuinely different top-level
    // domains — different "sites" per browser same-site rules — but both
    // call the same api.klarsocial.eu backend. That makes every request
    // cross-site, so SameSite=Lax is silently dropped by browsers on
    // fetch()/XHR. SameSite=None (which requires Secure, already handled
    // above) is required for cross-site cookies to actually be sent.
    let same_site = if is_prod { "None" } else { "Lax" };

    let mut headers = HeaderMap::new();

    headers.insert(
        axum::http::header::SET_COOKIE,
        HeaderValue::from_str(&format!(
            "klar_access_token={}; HttpOnly; {}SameSite={}; Path=/; Max-Age=900",
            access, secure_flag, same_site
        )).unwrap(),
    );

    headers.append(
        axum::http::header::SET_COOKIE,
        HeaderValue::from_str(&format!(
            "klar_refresh_token={}; HttpOnly; {}SameSite={}; Path=/; Max-Age=2592000",
            refresh, secure_flag, same_site
        )).unwrap(),
    );

    headers
}

fn build_clear_cookies() -> HeaderMap {
    let is_prod = std::env::var("ENV").unwrap_or_default() == "production";
    let secure_flag = if is_prod { "Secure; " } else { "" };
    let same_site = if is_prod { "None" } else { "Lax" };

    let mut headers = HeaderMap::new();
    headers.insert(
        axum::http::header::SET_COOKIE,
        HeaderValue::from_str(&format!("klar_access_token=; HttpOnly; {}SameSite={}; Path=/; Max-Age=0", secure_flag, same_site)).unwrap(),
    );
    headers.append(
        axum::http::header::SET_COOKIE,
        HeaderValue::from_str(&format!("klar_refresh_token=; HttpOnly; {}SameSite={}; Path=/; Max-Age=0", secure_flag, same_site)).unwrap(),
    );
    headers
}

/// Generate a secure random hex token for email verification/password reset
fn generate_email_token() -> String {
    let bytes: [u8; 32] = rand::rng().random();
    hex::encode(bytes)
}

/// Store a refresh token in the database and return the raw token for the client
async fn create_and_store_refresh_token(
    pool: &sqlx::PgPool,
    user_id: uuid::Uuid,
    device_info: Option<&str>,
) -> Result<String, AppError> {
    let raw_token = generate_refresh_token();
    let token_hash = hash_refresh_token(&raw_token);

    sqlx::query(
        r#"
        INSERT INTO refresh_tokens (user_id, token_hash, device_info, expires_at)
        VALUES ($1, $2, $3, NOW() + INTERVAL '30 days')
        "#
    )
    .bind(user_id)
    .bind(&token_hash)
    .bind(device_info)
    .execute(pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to store refresh token: {}", e);
        AppError::internal("Failed to create session")
    })?;

    Ok(raw_token)
}

/// POST /auth/register
pub async fn register(
    State(state): State<AppState>,
    Json(input): Json<RegisterRequest>,
) -> Result<(StatusCode, HeaderMap, Json<AuthResponse>), AppError> {

    if input.username.is_empty() || input.email.is_empty() || input.password.is_empty() {
        return Err(AppError::bad_request("All fields are required"));
    }

    // Case is preserved exactly as entered -- uniqueness and lookups are
    // case-insensitive (see idx_users_username_ci), not the stored value.
    let username = input.username.trim().to_string();

    if username.len() > 30 {
        return Err(AppError::bad_request("Username must be 30 characters or less"));
    }

    if input.password.len() < 8 {
        return Err(AppError::bad_request("Password must be at least 8 characters"));
    }

    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let password_hash = argon2
        .hash_password(input.password.as_bytes(), &salt)
        .map_err(|_| AppError::internal("Failed to hash password"))?
        .to_string();

    let user = sqlx::query_as::<_, UserRow>(
        "INSERT INTO users (username, email, password_hash) VALUES ($1, $2, $3) RETURNING *"
    )
    .bind(&username)
    .bind(&input.email)
    .bind(&password_hash)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        let msg = e.to_string();
        if msg.contains("duplicate key") {
            AppError::conflict("Username or email already taken")
        } else {
            tracing::error!("Failed to register user: {}", msg);
            AppError::internal("Failed to register user")
        }
    })?;

    tracing::info!("Registered user: {} ({})", user.username, user.id);

    // Send verification email
    let email_token = generate_email_token();
    sqlx::query(
        r#"
        INSERT INTO email_tokens (user_id, token, token_type, expires_at)
        VALUES ($1, $2, 'verification', NOW() + INTERVAL '24 hours')
        "#
    )
    .bind(user.id)
    .bind(&email_token)
    .execute(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Failed to create verification token: {}", e);
        AppError::internal("Failed to create verification token")
    })?;

    {
        let email_service = state.email.clone();
        let to_email = user.email.clone();
        let token = email_token.clone();
        tokio::spawn(async move {
            if let Err(e) = email_service.send_verification(&to_email, &token).await {
                tracing::error!("Failed to send verification email: {}", e);
            }
        });
    }

    // Create tokens
    let access_token = create_access_token(user.id, &state.jwt_secret)
        .map_err(|_| AppError::internal("Failed to create access token"))?;
    let refresh_token = create_and_store_refresh_token(&state.db, user.id, None).await?;

    Ok((
        StatusCode::CREATED,
        build_auth_cookies(&access_token, &refresh_token),
        Json(AuthResponse {
            access_token: "".to_string(), // Keep struct signature for backward compatibility
            refresh_token: "".to_string(),
            user: UserResponse::from(user),
        }),
    ))
}

/// POST /auth/login
pub async fn login(
    State(state): State<AppState>,
    Json(input): Json<LoginRequest>,
) -> Result<(HeaderMap, Json<AuthResponse>), AppError> {

    let user = sqlx::query_as::<_, UserRow>(
        "SELECT * FROM users WHERE email = $1"
    )
    .bind(&input.email)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Database error: {}", e);
        AppError::internal("Database error")
    })?;

    let user = user.ok_or_else(|| {
        AppError::bad_request("Invalid email or password")
    })?;

    let stored_hash = user.password_hash.as_ref().ok_or_else(|| {
        AppError::bad_request("Invalid email or password")
    })?;

    let parsed_hash = PasswordHash::new(stored_hash)
        .map_err(|_| AppError::internal("Failed to parse password hash"))?;

    Argon2::default()
        .verify_password(input.password.as_bytes(), &parsed_hash)
        .map_err(|_| AppError::bad_request("Invalid email or password"))?;

    let access_token = create_access_token(user.id, &state.jwt_secret)
        .map_err(|_| AppError::internal("Failed to create access token"))?;
    let refresh_token = create_and_store_refresh_token(&state.db, user.id, None).await?;

    tracing::info!("User logged in: {} ({})", user.username, user.id);

    Ok((
        build_auth_cookies(&access_token, &refresh_token),
        Json(AuthResponse {
            access_token: "".to_string(),
            refresh_token: "".to_string(),
            user: UserResponse::from(user),
        }),
    ))
}

/// POST /auth/refresh
pub async fn refresh(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<(HeaderMap, Json<RefreshResponse>), AppError> {

    let cookie_header = headers.get(axum::http::header::COOKIE).and_then(|v| v.to_str().ok());
    let raw_refresh_token = cookie_header
        .and_then(|h| h.split(';').map(|s| s.trim()).find(|s| s.starts_with("klar_refresh_token=")))
        .and_then(|s| s.strip_prefix("klar_refresh_token="))
        .ok_or_else(|| AppError::unauthorized("No refresh token found in cookies"))?;

    let token_hash = hash_refresh_token(raw_refresh_token);

    let token_row = sqlx::query_as::<_, (uuid::Uuid, uuid::Uuid)>(
        r#"
        SELECT id, user_id FROM refresh_tokens
        WHERE token_hash = $1 AND expires_at > NOW()
        "#
    )
    .bind(&token_hash)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Database error: {}", e);
        AppError::internal("Database error")
    })?;

    let (old_token_id, user_id) = token_row
        .ok_or_else(|| AppError::unauthorized("Invalid or expired refresh token"))?;

    sqlx::query("DELETE FROM refresh_tokens WHERE id = $1")
        .bind(old_token_id)
        .execute(&state.db)
        .await
        .map_err(|e| {
            tracing::error!("Failed to delete old refresh token: {}", e);
            AppError::internal("Database error")
        })?;

    let access_token = create_access_token(user_id, &state.jwt_secret)
        .map_err(|_| AppError::internal("Failed to create access token"))?;
    let new_refresh_token = create_and_store_refresh_token(&state.db, user_id, None).await?;

    tracing::info!("Token refreshed for user: {}", user_id);

    Ok((
        build_auth_cookies(&access_token, &new_refresh_token),
        Json(RefreshResponse {
            access_token: "".to_string(),
            refresh_token: "".to_string(),
        })
    ))
}

/// POST /auth/logout
pub async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<(HeaderMap, Json<serde_json::Value>), AppError> {

    let cookie_header = headers.get(axum::http::header::COOKIE).and_then(|v| v.to_str().ok());
    if let Some(raw_refresh_token) = cookie_header
        .and_then(|h| h.split(';').map(|s| s.trim()).find(|s| s.starts_with("klar_refresh_token=")))
        .and_then(|s| s.strip_prefix("klar_refresh_token="))
    {
        let token_hash = hash_refresh_token(raw_refresh_token);
        let _ = sqlx::query("DELETE FROM refresh_tokens WHERE token_hash = $1")
            .bind(&token_hash)
            .execute(&state.db)
            .await;
    }

    Ok((
        build_clear_cookies(),
        Json(serde_json::json!({ "message": "Logged out successfully" }))
    ))
}

#[derive(Deserialize)]
pub struct VerifyQuery {
    pub token: String,
}

/// GET /auth/verify?token=xxx
pub async fn verify_email(
    State(state): State<AppState>,
    Query(query): Query<VerifyQuery>,
) -> Result<Json<serde_json::Value>, AppError> {

    let token_row = sqlx::query_as::<_, (uuid::Uuid, uuid::Uuid)>(
        r#"
        SELECT id, user_id FROM email_tokens
        WHERE token = $1
          AND token_type = 'verification'
          AND used_at IS NULL
          AND expires_at > NOW()
        "#
    )
    .bind(&query.token)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Database error: {}", e);
        AppError::internal("Database error")
    })?;

    let (token_id, user_id) = token_row
        .ok_or_else(|| AppError::bad_request("Invalid or expired verification link"))?;

    sqlx::query("UPDATE users SET email_verified = TRUE WHERE id = $1")
        .bind(user_id)
        .execute(&state.db)
        .await
        .map_err(|e| {
            tracing::error!("Failed to verify email: {}", e);
            AppError::internal("Failed to verify email")
        })?;

    sqlx::query("UPDATE email_tokens SET used_at = NOW() WHERE id = $1")
        .bind(token_id)
        .execute(&state.db)
        .await
        .map_err(|e| {
            tracing::error!("Failed to mark token as used: {}", e);
            AppError::internal("Failed to mark token as used")
        })?;

    tracing::info!("Email verified for user: {}", user_id);

    Ok(Json(serde_json::json!({
        "message": "Email verified successfully"
    })))
}

#[derive(Deserialize)]
pub struct ForgotPasswordRequest {
    pub email: String,
}

/// POST /auth/forgot-password
pub async fn forgot_password(
    State(state): State<AppState>,
    Json(input): Json<ForgotPasswordRequest>,
) -> Result<Json<serde_json::Value>, AppError> {

    let user = sqlx::query_as::<_, UserRow>(
        "SELECT * FROM users WHERE email = $1"
    )
    .bind(&input.email)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Database error: {}", e);
        AppError::internal("Database error")
    })?;

    if let Some(user) = user {
        sqlx::query(
            "UPDATE email_tokens SET used_at = NOW() WHERE user_id = $1 AND token_type = 'password_reset' AND used_at IS NULL"
        )
        .bind(user.id)
        .execute(&state.db)
        .await
        .map_err(|e| {
            tracing::error!("Failed to invalidate old tokens: {}", e);
            AppError::internal("Database error")
        })?;

        let token = generate_email_token();
        sqlx::query(
            r#"
            INSERT INTO email_tokens (user_id, token, token_type, expires_at)
            VALUES ($1, $2, 'password_reset', NOW() + INTERVAL '1 hour')
            "#
        )
        .bind(user.id)
        .bind(&token)
        .execute(&state.db)
        .await
        .map_err(|e| {
            tracing::error!("Failed to create reset token: {}", e);
            AppError::internal("Failed to create reset token")
        })?;

        {
            let email_service = state.email.clone();
            let to_email = user.email.clone();
            let reset_token = token.clone();
            tokio::spawn(async move {
                if let Err(e) = email_service.send_password_reset(&to_email, &reset_token).await {
                    tracing::error!("Failed to send reset email: {}", e);
                }
            });
        }
    }

    Ok(Json(serde_json::json!({
        "message": "If an account with that email exists, a reset link has been sent"
    })))
}

#[derive(Deserialize)]
pub struct ResetPasswordRequest {
    pub token: String,
    pub new_password: String,
}

/// POST /auth/reset-password
pub async fn reset_password(
    State(state): State<AppState>,
    Json(input): Json<ResetPasswordRequest>,
) -> Result<Json<serde_json::Value>, AppError> {

    if input.new_password.len() < 8 {
        return Err(AppError::bad_request("Password must be at least 8 characters"));
    }

    let token_row = sqlx::query_as::<_, (uuid::Uuid, uuid::Uuid)>(
        r#"
        SELECT id, user_id FROM email_tokens
        WHERE token = $1
          AND token_type = 'password_reset'
          AND used_at IS NULL
          AND expires_at > NOW()
        "#
    )
    .bind(&input.token)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Database error: {}", e);
        AppError::internal("Database error")
    })?;

    let (token_id, user_id) = token_row
        .ok_or_else(|| AppError::bad_request("Invalid or expired reset link"))?;

    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let password_hash = argon2
        .hash_password(input.new_password.as_bytes(), &salt)
        .map_err(|_| AppError::internal("Failed to hash password"))?
        .to_string();

    sqlx::query("UPDATE users SET password_hash = $1 WHERE id = $2")
        .bind(&password_hash)
        .bind(user_id)
        .execute(&state.db)
        .await
        .map_err(|e| {
            tracing::error!("Failed to update password: {}", e);
            AppError::internal("Failed to update password")
        })?;

    sqlx::query("UPDATE email_tokens SET used_at = NOW() WHERE id = $1")
        .bind(token_id)
        .execute(&state.db)
        .await
        .map_err(|e| {
            tracing::error!("Failed to mark token as used: {}", e);
            AppError::internal("Database error")
        })?;

    sqlx::query("DELETE FROM refresh_tokens WHERE user_id = $1")
        .bind(user_id)
        .execute(&state.db)
        .await
        .map_err(|e| {
            tracing::error!("Failed to invalidate sessions: {}", e);
            AppError::internal("Database error")
        })?;

    tracing::info!("Password reset for user: {}", user_id);

    Ok(Json(serde_json::json!({
        "message": "Password reset successfully. Please log in again."
    })))
}

/// POST /auth/resend-verification
pub async fn resend_verification(
    State(state): State<AppState>,
    auth: crate::auth::AuthUser,
) -> Result<Json<serde_json::Value>, AppError> {

    let user = sqlx::query_as::<_, UserRow>(
        "SELECT * FROM users WHERE id = $1"
    )
    .bind(auth.user_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Database error: {}", e);
        AppError::internal("Database error")
    })?;

    if user.email_verified {
        return Err(AppError::bad_request("Email is already verified"));
    }

    sqlx::query(
        "UPDATE email_tokens SET used_at = NOW() WHERE user_id = $1 AND token_type = 'verification' AND used_at IS NULL"
    )
    .bind(user.id)
    .execute(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Failed to invalidate old tokens: {}", e);
        AppError::internal("Database error")
    })?;

    let token = generate_email_token();
    sqlx::query(
        r#"
        INSERT INTO email_tokens (user_id, token, token_type, expires_at)
        VALUES ($1, $2, 'verification', NOW() + INTERVAL '24 hours')
        "#
    )
    .bind(user.id)
    .bind(&token)
    .execute(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Failed to create verification token: {}", e);
        AppError::internal("Failed to create verification token")
    })?;

    {
        let email_service = state.email.clone();
        let to_email = user.email.clone();
        let verify_token = token.clone();
        tokio::spawn(async move {
            if let Err(e) = email_service.send_verification(&to_email, &verify_token).await {
                tracing::error!("Failed to send verification email: {}", e);
            }
        });
    }

    Ok(Json(serde_json::json!({
        "message": "Verification email sent"
    })))
}
