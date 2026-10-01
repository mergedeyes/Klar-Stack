//! Auth handlers — registration, login, refresh, logout, email verification, password reset.

use axum::{
    extract::{MatchedPath, Query, Request, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use argon2::{
    password_hash::{rand_core::OsRng, SaltString, PasswordHasher, PasswordHash, PasswordVerifier},
    Argon2,
};
use rand::Rng;
use serde::Deserialize;

use crate::auth::{cookie_value, create_access_token, generate_refresh_token, hash_refresh_token, OptionalAuthUser};
use crate::email::EmailService;
use crate::errors::AppError;
use crate::models::{
    AuthResponse, LoginRequest, RefreshResponse,
    RegisterRequest, UserResponse, UserRow,
};
use crate::storage::{CdnPurger, EvidenceStorage, Storage};
use crate::utils::{DbResultExt, ResolveMedia};
use crate::validation::{normalize_email, validate_new_email, validate_password, validate_username};

#[derive(Clone)]
pub struct AppState {
    pub db: sqlx::PgPool,
    pub jwt_secret: String,
    pub storage: Storage,
    pub cdn: CdnPurger,
    // Separate zone for preserved evidence (see evidence.rs).
    pub evidence: EvidenceStorage,
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

/// Store a refresh token in the database and return the raw token for the
/// client. `family`: the login it continues, for a rotation; None starts a
/// new family (a login).
async fn create_and_store_refresh_token(
    pool: &sqlx::PgPool,
    user_id: uuid::Uuid,
    family: Option<uuid::Uuid>,
) -> Result<String, AppError> {
    let raw_token = generate_refresh_token();
    let token_hash = hash_refresh_token(&raw_token);

    sqlx::query(
        r#"
        INSERT INTO refresh_tokens (user_id, token_hash, expires_at, family_id)
        VALUES ($1, $2, NOW() + INTERVAL '30 days', COALESCE($3, uuid_generate_v7()))
        "#
    )
    .bind(user_id)
    .bind(&token_hash)
    .bind(family)
    .execute(pool)
    .await
    .db_err_ctx("Failed to store refresh token", "Failed to create session")?;

    Ok(raw_token)
}

/// A new session: an access token and a refresh token in a new family, and
/// the cookies that carry them for same-site clients. Returns (cookies,
/// access token, refresh token).
pub(crate) async fn new_session(state: &AppState, user_id: uuid::Uuid) -> Result<(HeaderMap, String, String), AppError> {
    let access_token = create_access_token(user_id, &state.jwt_secret)
        .map_err(|_| AppError::internal("Failed to create access token"))?;
    let refresh_token = create_and_store_refresh_token(&state.db, user_id, None).await?;
    Ok((auth_cookie_headers(Some((&access_token, &refresh_token))), access_token, refresh_token))
}

/// How long after its rotation a refresh token may turn up again without
/// counting as stolen: two tabs of the same browser can refresh with the
/// same token at once, and the loser picks up the winner's new tokens (see
/// klar-web's api.ts).
const REFRESH_REUSE_GRACE_SECS: f64 = 30.0;

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
    let mut conn = state.db.acquire().await.db_err("Database error")?;
    let email_token = create_verification_token(&mut conn, user.id, 24).await?;
    drop(conn);

    {
        let email_service = state.email.clone();
        let to_email = user.email.clone();
        let username = user.username.clone();
        let token = email_token.clone();
        tokio::spawn(async move {
            if let Err(e) = email_service.send_verification(&to_email, &username, &token).await {
                tracing::error!("Failed to send verification email: {}", e);
            }
        });
    }

    let (cookies, access_token, refresh_token) = new_session(&state, user.id).await?;

    Ok((
        StatusCode::CREATED,
        cookies,
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
    verify_password(&user, &input.password)?;

    // Only after the password checks out, so the lock can't be used to
    // find out which accounts are locked.
    crate::handlers::account_lock::refuse_if_locked(&state.db, user.id).await?;

    let (cookies, access_token, refresh_token) = new_session(&state, user.id).await?;

    tracing::info!("User logged in: {} ({})", user.username, user.id);

    Ok((
        cookies,
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

    // Rotate the token in a single statement: it is marked rotated rather
    // than deleted, and its successor joins the same family (the login it
    // started from). The `rotated_at IS NULL` guard makes it single-use
    // even when two refreshes race; only one of them gets the row back.
    let rotated = sqlx::query_as::<_, (uuid::Uuid, uuid::Uuid)>(
        r#"
        UPDATE refresh_tokens SET rotated_at = NOW()
        WHERE token_hash = $1 AND rotated_at IS NULL AND expires_at > NOW()
        RETURNING user_id, family_id
        "#
    )
    .bind(&token_hash)
    .fetch_optional(&state.db)
    .await
    .db_err("Database error")?;

    let Some((user_id, family_id)) = rotated else {
        // An already rotated token turning up again, after the grace period
        // for racing tabs: someone else has a copy. Either the thief came
        // second, or the thief came first and this is the real owner -- we
        // can't tell, so every session of that login ends.
        let reused = sqlx::query_scalar::<_, uuid::Uuid>(
            r#"
            DELETE FROM refresh_tokens WHERE family_id = (
                SELECT family_id FROM refresh_tokens
                WHERE token_hash = $1 AND rotated_at < NOW() - make_interval(secs => $2)
            )
            RETURNING family_id
            "#,
        )
        .bind(&token_hash)
        .bind(REFRESH_REUSE_GRACE_SECS)
        .fetch_all(&state.db)
        .await
        .db_err("Database error")?;
        if let Some(family) = reused.first() {
            tracing::warn!("Refresh token reused after rotation: family {} revoked", family);
        }
        return Err(AppError::unauthorized("Invalid or expired refresh token"));
    };

    let access_token = create_access_token(user_id, &state.jwt_secret)
        .map_err(|_| AppError::internal("Failed to create access token"))?;
    let new_refresh_token = create_and_store_refresh_token(&state.db, user_id, Some(family_id)).await?;

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

    // Ends this login: the token and the rotated ones it descends from.
    if let Some(raw_refresh_token) = cookie_token.or(body.refresh_token) {
        let token_hash = hash_refresh_token(&raw_refresh_token);
        let _ = sqlx::query(
            "DELETE FROM refresh_tokens WHERE family_id = (SELECT family_id FROM refresh_tokens WHERE token_hash = $1)",
        )
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

/// One reset email per account per this many minutes: every new link
/// invalidates the last, so a flood of requests from many IPs could
/// otherwise keep someone from ever using theirs.
const RESET_EMAIL_INTERVAL_MINUTES: i32 = 5;

/// POST /auth/forgot-password
///
/// Answers at once, with the same message for every address: the lookup
/// and the email happen afterwards, so neither the answer nor how long it
/// takes tells whether an account exists.
pub async fn forgot_password(
    State(state): State<AppState>,
    Json(input): Json<ForgotPasswordRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let email = normalize_email(&input.email);
    tokio::spawn(async move {
        if let Err(e) = send_reset_link(&state, &email).await {
            tracing::error!("Password reset request failed: {}", e.message);
        }
    });

    Ok(Json(serde_json::json!({
        "message": "If an account with that email exists, a reset link has been sent"
    })))
}

async fn send_reset_link(state: &AppState, email: &str) -> Result<(), AppError> {
    let mut tx = state.db.begin().await.db_err("Database error")?;
    // Locked, so two requests at once can't both pass the interval check.
    let Some((user_id, to_email)) = sqlx::query_as::<_, (uuid::Uuid, String)>(
        "SELECT id, email FROM users WHERE LOWER(email) = $1 FOR UPDATE",
    )
    .bind(email)
    .fetch_optional(&mut *tx)
    .await
    .db_err("Database error")?
    else {
        return Ok(());
    };

    let recently_sent = sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS(SELECT 1 FROM email_tokens
                      WHERE user_id = $1 AND token_type = 'password_reset'
                        AND created_at > NOW() - make_interval(mins => $2))
        "#,
    )
    .bind(user_id)
    .bind(RESET_EMAIL_INTERVAL_MINUTES)
    .fetch_one(&mut *tx)
    .await
    .db_err("Database error")?;
    if recently_sent {
        return Ok(());
    }

    sqlx::query(
        "UPDATE email_tokens SET used_at = NOW() WHERE user_id = $1 AND token_type = 'password_reset' AND used_at IS NULL"
    )
    .bind(user_id)
    .execute(&mut *tx)
    .await
    .db_err_ctx("Failed to invalidate old tokens", "Database error")?;
    let token = create_reset_token(&mut tx, user_id, 1).await?;
    tx.commit().await.db_err("Database error")?;

    if let Err(e) = state.email.send_password_reset(&to_email, &token).await {
        tracing::error!("Failed to send reset email: {}", e);
    }
    Ok(())
}

#[derive(Deserialize)]
pub struct ResetPasswordRequest {
    pub token: String,
    pub new_password: String,
}

/// Stores a new single-use password reset token for the user, valid for
/// `hours`, and returns it. Earlier unused ones stay valid until they
/// expire; forgot_password invalidates them itself.
pub async fn create_reset_token(conn: &mut sqlx::PgConnection, user_id: uuid::Uuid, hours: i32) -> Result<String, AppError> {
    let token = generate_email_token();
    sqlx::query(
        r#"
        INSERT INTO email_tokens (user_id, token, token_type, expires_at)
        VALUES ($1, $2, 'password_reset', NOW() + make_interval(hours => $3))
        "#
    )
    .bind(user_id)
    .bind(&token)
    .bind(hours)
    .execute(&mut *conn)
    .await
    .db_err("Failed to create reset token")?;
    Ok(token)
}

/// Checks a login password against the user's stored hash. The same
/// message for a wrong password as for an unknown email.
pub fn verify_password(user: &UserRow, password: &str) -> Result<(), AppError> {
    let stored_hash = user.password_hash.as_ref().ok_or_else(|| {
        AppError::bad_request("Invalid email or password")
    })?;
    let parsed_hash = PasswordHash::new(stored_hash)
        .map_err(|_| AppError::internal("Failed to parse password hash"))?;
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .map_err(|_| AppError::bad_request("Invalid email or password"))
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

    // Every session ends, access tokens included: whoever had the old
    // password may still be signed in.
    crate::handlers::account_lock::revoke_sessions(&mut tx, user_id).await?;

    // A new password set through an emailed link is what a lock waits for.
    crate::handlers::account_lock::unlock_after_reset(&mut tx, user_id).await?;

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

    let user = sqlx::query_as::<_, (uuid::Uuid, String, String, bool)>(
        "SELECT id, email, username, email_verified FROM users WHERE LOWER(email) = $1"
    )
    .bind(normalize_email(&input.email))
    .fetch_optional(&state.db)
    .await
    .db_err("Database error")?;

    let Some((user_id, to_email, username, email_verified)) = user else {
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

    let mut tx = state.db.begin().await.db_err_ctx("Failed to start transaction", "Database error")?;
    let token = create_verification_token(&mut tx, user_id, 24).await?;
    tx.commit().await.db_err_ctx("Failed to commit transaction", "Database error")?;

    let email_service = state.email.clone();
    tokio::spawn(async move {
        if let Err(e) = email_service.send_verification(&to_email, &username, &token).await {
            tracing::error!("Failed to send verification email: {}", e);
        }
    });

    Ok(generic)
}

/// A new verification link, valid for `hours`; earlier unused ones stop
/// working, so only the newest email's link counts.
pub(crate) async fn create_verification_token(
    conn: &mut sqlx::PgConnection,
    user_id: uuid::Uuid,
    hours: i32,
) -> Result<String, AppError> {
    sqlx::query(
        "UPDATE email_tokens SET used_at = NOW() WHERE user_id = $1 AND token_type = 'verification' AND used_at IS NULL"
    )
    .bind(user_id)
    .execute(&mut *conn)
    .await
    .db_err_ctx("Failed to invalidate old tokens", "Database error")?;

    let token = generate_email_token();
    sqlx::query(
        r#"
        INSERT INTO email_tokens (user_id, token, token_type, expires_at)
        VALUES ($1, $2, 'verification', NOW() + make_interval(hours => $3))
        "#
    )
    .bind(user_id)
    .bind(&token)
    .bind(hours)
    .execute(&mut *conn)
    .await
    .db_err("Failed to create verification token")?;
    Ok(token)
}

/// Writes that need a verified email address: publishing posts and
/// comments, sending messages and reporting. An unverified address may be
/// a typo -- someone else's inbox -- and an account nobody can reach is too
/// cheap a way to spam, or to flood the report queue and with it the
/// admins' urgent alerts. Reading, settings, likes, follows and blocks stay
/// open, and notices without an account go through the public form.
fn needs_verified_email(method: &Method, path: &str) -> bool {
    matches!(
        (method.as_str(), path),
        ("POST", "/posts")
            | ("POST", "/posts/upload")
            | ("POST", "/posts/{post_id}/comments")
            | ("POST", "/chats/send")
            | ("POST", "/reports")
    )
}

/// Middleware: refuses those writes until the address is verified. A
/// route_layer, so it sees the matched route template.
pub async fn enforce_verified_email(
    State(state): State<AppState>,
    auth: OptionalAuthUser,
    matched: Option<MatchedPath>,
    req: Request,
    next: Next,
) -> Response {
    let path = matched.as_ref().map(|m| m.as_str()).unwrap_or_else(|| req.uri().path());
    if let Some(user_id) = auth.user_id {
        if needs_verified_email(req.method(), path) {
            let verified = sqlx::query_scalar::<_, bool>("SELECT email_verified FROM users WHERE id = $1")
                .bind(user_id)
                .fetch_optional(&state.db)
                .await;
            match verified {
                Ok(Some(true)) => {}
                Ok(Some(false)) => {
                    return AppError::forbidden(
                        "Please verify your email address first: open the link we emailed you, or request a new one.",
                    )
                    .into_response()
                }
                Ok(None) => return AppError::unauthorized("Session ended").into_response(),
                Err(e) => return AppError::internal(format!("Database error: {}", e)).into_response(),
            }
        }
    }
    next.run(req).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publishing_messaging_and_reporting_need_a_verified_address() {
        assert!(needs_verified_email(&Method::POST, "/posts/upload"));
        assert!(needs_verified_email(&Method::POST, "/posts/{post_id}/comments"));
        assert!(needs_verified_email(&Method::POST, "/chats/send"));
        assert!(needs_verified_email(&Method::POST, "/reports"));
        assert!(!needs_verified_email(&Method::POST, "/posts/{post_id}/like"));
        assert!(!needs_verified_email(&Method::PATCH, "/users/me"));
        assert!(!needs_verified_email(&Method::GET, "/posts/{post_id}/comments"));
    }

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
