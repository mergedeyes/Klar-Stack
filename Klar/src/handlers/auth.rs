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

use crate::auth::{cookie_value, create_access_token, generate_refresh_token, hash_refresh_token};
use crate::email::EmailService;
use crate::errors::AppError;
use crate::models::{
    AuthResponse, LoginRequest, RefreshResponse,
    RegisterRequest, UserResponse, UserRow,
};
use crate::storage::{CdnPurger, Storage};
use crate::utils::{DbResultExt, ResolveMedia};
use crate::validation::{normalize_email, validate_new_email, validate_password, validate_username};

#[derive(Clone)]
pub struct AppState {
    pub db: sqlx::PgPool,
    pub jwt_secret: String,
    pub storage: Storage,
    pub cdn: CdnPurger,
    pub email: EmailService,
    // Local, in-process fan-out to this replica's own SSE subscribers.
    // Never written to directly from request handlers anymore — always go
    // through handlers::notifications::publish_notification(), which
    // PUBLISHes to Redis first so every replica (not just this one) ends
    // up delivering to its own local subscribers via this same channel.
    pub notification_tx: tokio::sync::broadcast::Sender<crate::handlers::notifications::NotificationEvent>,
    // Cheap to clone, auto-reconnects — safe to hand a clone to every
    // request handler that needs to PUBLISH a notification.
    pub redis: redis::aio::ConnectionManager,
}

/// Set-Cookie headers for the two auth cookies: `Some((access, refresh))`
/// sets them, `None` clears them (empty value, Max-Age=0) on logout. One
/// function for both so the attributes can't drift apart -- a clearing
/// cookie whose attributes differ from the original may not replace it.
fn auth_cookie_headers(tokens: Option<(&str, &str)>) -> HeaderMap {
    // Prüfen, ob wir in Produktion sind (z.B. über eine ENV-Variable)
    let is_prod = std::env::var("ENV").unwrap_or_default() == "production";

    // Lokal lassen wir "Secure" weg, online erzwingen wir es.
    let secure_flag = if is_prod { "Secure; " } else { "" };

    // klarsocial.eu and klarsocial.de are genuinely different top-level
    // domains — different "sites" per browser same-site rules — but both
    // call the same api.klarsocial.eu backend. That makes every request
    // cross-site. SameSite=None (paired with Secure) is required for the
    // cookie to even be considered — but a growing share of browsers
    // (privacy-hardened Chromium forks, Safari ITP, Firefox ETP) block
    // third-party cookies outright regardless of these attributes. Cookies
    // are kept here as a best-effort/same-site convenience, but the real
    // auth path for cross-site clients is the Authorization: Bearer header
    // (see AuthResponse/RefreshResponse below, and auth.rs's extractor).
    let same_site = if is_prod { "None" } else { "Lax" };

    // Max-Age matches the token lifetimes: 15 minutes for the access token
    // (auth.rs), 30 days for the refresh token (create_and_store_refresh_token).
    let (access, refresh, access_max_age, refresh_max_age) = match tokens {
        Some((access, refresh)) => (access, refresh, 900, 2_592_000),
        None => ("", "", 0, 0),
    };

    let mut headers = HeaderMap::new();
    for (name, value, max_age) in [
        ("klar_access_token", access, access_max_age),
        ("klar_refresh_token", refresh, refresh_max_age),
    ] {
        headers.append(
            axum::http::header::SET_COOKIE,
            HeaderValue::from_str(&format!(
                "{name}={value}; HttpOnly; {secure_flag}SameSite={same_site}; Path=/; Max-Age={max_age}"
            )).unwrap(),
        );
    }

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
    .db_err_ctx("Failed to store refresh token", "Failed to create session")?;

    Ok(raw_token)
}

/// POST /auth/register
pub async fn register(
    State(state): State<AppState>,
    Json(input): Json<RegisterRequest>,
) -> Result<(StatusCode, HeaderMap, Json<AuthResponse>), AppError> {

    // ToS/Privacy Policy consent -- enforced server-side, not just by the
    // frontend form's checkbox, so a direct POST /auth/register can't skip
    // it. See models/user.rs's RegisterRequest and migration
    // 20260825000000_add_terms_accepted_at.sql for the full rationale.
    if !input.accept_terms {
        return Err(AppError::bad_request(
            "You must accept the Terms of Service and Privacy Policy to register"
        ));
    }

    // Case is preserved exactly as entered -- uniqueness and lookups are
    // case-insensitive (see idx_users_username_ci), not the stored value.
    let username = validate_username(&input.username)?;
    let email = validate_new_email(&input.email)?;
    validate_password(&input.password)?;

    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let password_hash = argon2
        .hash_password(input.password.as_bytes(), &salt)
        .map_err(|_| AppError::internal("Failed to hash password"))?
        .to_string();

    let user = sqlx::query_as::<_, UserRow>(
        "INSERT INTO users (username, email, password_hash, terms_accepted_at) VALUES ($1, $2, $3, NOW()) RETURNING *"
    )
    .bind(&username)
    .bind(&email)
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
    .db_err("Failed to create verification token")?;

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
        auth_cookie_headers(Some((&access_token, &refresh_token))),
        Json(AuthResponse {
            // Returned in the body now (not blanked) so cross-site clients
            // that can't rely on third-party cookies can store these and
            // send them explicitly via Authorization: Bearer.
            access_token,
            refresh_token,
            user: UserResponse::from(user).resolve_media(&state.storage),
        }),
    ))
}

/// POST /auth/login
pub async fn login(
    State(state): State<AppState>,
    Json(input): Json<LoginRequest>,
) -> Result<(HeaderMap, Json<AuthResponse>), AppError> {

    let user = sqlx::query_as::<_, UserRow>(
        "SELECT * FROM users WHERE LOWER(email) = $1"
    )
    .bind(normalize_email(&input.email))
    .fetch_optional(&state.db)
    .await
    .db_err("Database error")?;

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
        auth_cookie_headers(Some((&access_token, &refresh_token))),
        Json(AuthResponse {
            access_token,
            refresh_token,
            user: UserResponse::from(user).resolve_media(&state.storage),
        }),
    ))
}

/// Body for /auth/refresh — refresh_token is optional here because same-site
/// clients rely on the httpOnly cookie instead; cross-site clients (blocked
/// from receiving that cookie by third-party cookie policies) send it explicitly.
/// Frontend always sends at least `{}` so this extractor never sees a truly
/// empty/missing body.
#[derive(Debug, Deserialize, Default)]
pub struct RefreshRequest {
    pub refresh_token: Option<String>,
}

/// POST /auth/refresh
pub async fn refresh(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<RefreshRequest>,
) -> Result<(HeaderMap, Json<RefreshResponse>), AppError> {

    let cookie_token = cookie_value(&headers, "klar_refresh_token").map(str::to_string);

    let raw_refresh_token = cookie_token
        .or(body.refresh_token)
        .ok_or_else(|| AppError::unauthorized("No refresh token found"))?;

    let token_hash = hash_refresh_token(&raw_refresh_token);

    // Consume the token in a single statement. A separate SELECT followed
    // by a DELETE let two concurrent refreshes with the same token both
    // pass the SELECT and both mint a new session; with DELETE .. RETURNING
    // only one of them can get the row back.
    let user_id = sqlx::query_scalar::<_, uuid::Uuid>(
        r#"
        DELETE FROM refresh_tokens
        WHERE token_hash = $1 AND expires_at > NOW()
        RETURNING user_id
        "#
    )
    .bind(&token_hash)
    .fetch_optional(&state.db)
    .await
    .db_err("Database error")?
    .ok_or_else(|| AppError::unauthorized("Invalid or expired refresh token"))?;

    let access_token = create_access_token(user_id, &state.jwt_secret)
        .map_err(|_| AppError::internal("Failed to create access token"))?;
    let new_refresh_token = create_and_store_refresh_token(&state.db, user_id, None).await?;

    tracing::info!("Token refreshed for user: {}", user_id);

    Ok((
        auth_cookie_headers(Some((&access_token, &new_refresh_token))),
        Json(RefreshResponse {
            access_token,
            refresh_token: new_refresh_token,
        })
    ))
}

/// Body for /auth/logout — same rationale as RefreshRequest. Frontend always
/// sends at least `{}`, so this is required (not Option<Json<..>>) to avoid
/// depending on axum's optional-body extractor support.
#[derive(Debug, Deserialize, Default)]
pub struct LogoutRequest {
    pub refresh_token: Option<String>,
}

/// POST /auth/logout
pub async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<LogoutRequest>,
) -> Result<(HeaderMap, Json<serde_json::Value>), AppError> {

    let cookie_token = cookie_value(&headers, "klar_refresh_token").map(str::to_string);

    if let Some(raw_refresh_token) = cookie_token.or(body.refresh_token) {
        let token_hash = hash_refresh_token(&raw_refresh_token);
        let _ = sqlx::query("DELETE FROM refresh_tokens WHERE token_hash = $1")
            .bind(&token_hash)
            .execute(&state.db)
            .await;
    }

    Ok((
        auth_cookie_headers(None),
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

    // Consume the token and verify in one transaction; UPDATE .. RETURNING
    // makes the token single-use even under concurrent requests.
    let mut tx = state.db.begin().await.db_err_ctx("Failed to start transaction", "Database error")?;

    let user_id = sqlx::query_scalar::<_, uuid::Uuid>(
        r#"
        UPDATE email_tokens SET used_at = NOW()
        WHERE token = $1
          AND token_type = 'verification'
          AND used_at IS NULL
          AND expires_at > NOW()
        RETURNING user_id
        "#
    )
    .bind(&query.token)
    .fetch_optional(&mut *tx)
    .await
    .db_err("Database error")?
    .ok_or_else(|| AppError::bad_request("Invalid or expired verification link"))?;

    sqlx::query("UPDATE users SET email_verified = TRUE WHERE id = $1")
        .bind(user_id)
        .execute(&mut *tx)
        .await
        .db_err("Failed to verify email")?;

    tx.commit().await.db_err_ctx("Failed to commit transaction", "Database error")?;

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
        "SELECT * FROM users WHERE LOWER(email) = $1"
    )
    .bind(normalize_email(&input.email))
    .fetch_optional(&state.db)
    .await
    .db_err("Database error")?;

    if let Some(user) = user {
        sqlx::query(
            "UPDATE email_tokens SET used_at = NOW() WHERE user_id = $1 AND token_type = 'password_reset' AND used_at IS NULL"
        )
        .bind(user.id)
        .execute(&state.db)
        .await
        .db_err_ctx("Failed to invalidate old tokens", "Database error")?;

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
        .db_err("Failed to create reset token")?;

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

    validate_password(&input.new_password)?;

    // Hash before touching the token, so a hashing failure can't burn a
    // valid reset link.
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let password_hash = argon2
        .hash_password(input.new_password.as_bytes(), &salt)
        .map_err(|_| AppError::internal("Failed to hash password"))?
        .to_string();

    // Consume the token, set the password and revoke every session as one
    // unit. UPDATE .. RETURNING makes the link single-use even if it's
    // submitted twice concurrently.
    let mut tx = state.db.begin().await.db_err_ctx("Failed to start transaction", "Database error")?;

    let user_id = sqlx::query_scalar::<_, uuid::Uuid>(
        r#"
        UPDATE email_tokens SET used_at = NOW()
        WHERE token = $1
          AND token_type = 'password_reset'
          AND used_at IS NULL
          AND expires_at > NOW()
        RETURNING user_id
        "#
    )
    .bind(&input.token)
    .fetch_optional(&mut *tx)
    .await
    .db_err("Database error")?
    .ok_or_else(|| AppError::bad_request("Invalid or expired reset link"))?;

    sqlx::query("UPDATE users SET password_hash = $1 WHERE id = $2")
        .bind(&password_hash)
        .bind(user_id)
        .execute(&mut *tx)
        .await
        .db_err("Failed to update password")?;

    sqlx::query("DELETE FROM refresh_tokens WHERE user_id = $1")
        .bind(user_id)
        .execute(&mut *tx)
        .await
        .db_err_ctx("Failed to invalidate sessions", "Database error")?;

    tx.commit().await.db_err_ctx("Failed to commit transaction", "Database error")?;

    tracing::info!("Password reset for user: {}", user_id);

    Ok(Json(serde_json::json!({
        "message": "Password reset successfully. Please log in again."
    })))
}

#[derive(Deserialize)]
pub struct ResendVerificationRequest {
    pub email: String,
}

/// POST /auth/resend-verification
///
/// Email-based and unauthenticated, like forgot-password: the page that
/// calls it is reached from an expired verification link, typically while
/// logged out. (It used to require a login and ignore the email, so it
/// failed for exactly the people who needed it.) Always answers with the
/// same generic message so it can't be used to probe which emails are
/// registered or verified.
///
/// Besides the per-IP auth rate limit, each account gets at most one
/// verification email per minute, so this can't be used to flood
/// someone's inbox from many IPs.
pub async fn resend_verification(
    State(state): State<AppState>,
    Json(input): Json<ResendVerificationRequest>,
) -> Result<Json<serde_json::Value>, AppError> {

    let generic = Json(serde_json::json!({
        "message": "If an unverified account exists for that email, a new verification link has been sent"
    }));

    let user = sqlx::query_as::<_, (uuid::Uuid, String, bool)>(
        "SELECT id, email, email_verified FROM users WHERE LOWER(email) = $1"
    )
    .bind(normalize_email(&input.email))
    .fetch_optional(&state.db)
    .await
    .db_err("Database error")?;

    let Some((user_id, to_email, email_verified)) = user else {
        return Ok(generic);
    };
    if email_verified {
        return Ok(generic);
    }

    let recently_sent = sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM email_tokens
            WHERE user_id = $1 AND token_type = 'verification'
              AND created_at > NOW() - INTERVAL '1 minute'
        )
        "#
    )
    .bind(user_id)
    .fetch_one(&state.db)
    .await
    .db_err("Database error")?;

    if recently_sent {
        return Ok(generic);
    }

    let token = generate_email_token();

    let mut tx = state.db.begin().await.db_err_ctx("Failed to start transaction", "Database error")?;

    sqlx::query(
        "UPDATE email_tokens SET used_at = NOW() WHERE user_id = $1 AND token_type = 'verification' AND used_at IS NULL"
    )
    .bind(user_id)
    .execute(&mut *tx)
    .await
    .db_err_ctx("Failed to invalidate old tokens", "Database error")?;

    sqlx::query(
        r#"
        INSERT INTO email_tokens (user_id, token, token_type, expires_at)
        VALUES ($1, $2, 'verification', NOW() + INTERVAL '24 hours')
        "#
    )
    .bind(user_id)
    .bind(&token)
    .execute(&mut *tx)
    .await
    .db_err("Failed to create verification token")?;

    tx.commit().await.db_err_ctx("Failed to commit transaction", "Database error")?;

    let email_service = state.email.clone();
    tokio::spawn(async move {
        if let Err(e) = email_service.send_verification(&to_email, &token).await {
            tracing::error!("Failed to send verification email: {}", e);
        }
    });

    Ok(generic)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set_cookies(headers: &HeaderMap) -> Vec<&str> {
        headers
            .get_all(axum::http::header::SET_COOKIE)
            .iter()
            .map(|v| v.to_str().unwrap())
            .collect()
    }

    // Outside production (ENV unset in tests): no Secure flag, SameSite=Lax.
    #[test]
    fn sets_both_cookies_with_token_lifetimes() {
        let headers = auth_cookie_headers(Some(("acc", "ref")));
        assert_eq!(set_cookies(&headers), [
            "klar_access_token=acc; HttpOnly; SameSite=Lax; Path=/; Max-Age=900",
            "klar_refresh_token=ref; HttpOnly; SameSite=Lax; Path=/; Max-Age=2592000",
        ]);
    }

    #[test]
    fn clears_both_cookies_with_same_attributes() {
        let headers = auth_cookie_headers(None);
        assert_eq!(set_cookies(&headers), [
            "klar_access_token=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0",
            "klar_refresh_token=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0",
        ]);
    }
}
