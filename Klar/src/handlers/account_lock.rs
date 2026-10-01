//! Locking an account an admin suspects was taken over (e.g. a normal
//! account suddenly posting spam), see the account_locks migration.
//!
//! Locking signs the account out everywhere at once: refresh tokens are
//! deleted, and `enforce_account_lock` rejects its still-valid access
//! tokens (they live up to 15 minutes, too long to leave an intruder
//! reading messages). The owner gets an email with a reset link; setting
//! a new password through any reset link unlocks the account. Until then,
//! logging in with the right password answers 423 Locked, and the login
//! page offers to send the link again (`resend_link`, at most every 15
//! minutes and 10 times per lock). The password is checked first, so the
//! lock never tells a stranger which accounts are locked. Whoever knows
//! the password can only send links to the owner's own inbox (users can't
//! change their address), so a resend is no use to an intruder.

use axum::{
    extract::{Path, Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::auth::{AuthUser, OptionalAuthUser};
use crate::errors::AppError;
use crate::handlers::auth::{create_reset_token, verify_password, AppState};
use crate::handlers::reports::require_admin;
use crate::models::UserRow;
use crate::utils::DbResultExt;
use crate::validation::normalize_email;

/// Reset links from a lock email or a resend stay valid this long: the
/// owner may not read the email for a while.
const LINK_HOURS: i32 = 24;
const RESEND_INTERVAL_MINUTES: i32 = 15;
const MAX_LINKS_PER_LOCK: i32 = 10;
const NOTE_MAX: usize = 2000;

async fn is_locked(conn: &mut PgConnection, user_id: Uuid) -> Result<bool, AppError> {
    sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM account_locks WHERE user_id = $1 AND unlocked_at IS NULL)")
        .bind(user_id)
        .fetch_one(&mut *conn)
        .await
        .db_err("Database error")
}

fn locked_error() -> AppError {
    AppError {
        status: StatusCode::LOCKED,
        message: "We suspect someone else has been using this account, so we locked it and emailed you a link \
                  to set a new password. Setting one unlocks the account."
            .into(),
    }
}

/// For login, after the password was checked.
pub async fn refuse_if_locked(db: &sqlx::PgPool, user_id: Uuid) -> Result<(), AppError> {
    let mut conn = db.acquire().await.db_err("Database error")?;
    if is_locked(&mut conn, user_id).await? {
        return Err(locked_error());
    }
    Ok(())
}

/// For reset_password, inside its transaction.
pub async fn unlock_after_reset(tx: &mut PgConnection, user_id: Uuid) -> Result<(), AppError> {
    let unlocked = sqlx::query(
        "UPDATE account_locks SET unlocked_at = NOW(), unlocked_via = 'password_reset' WHERE user_id = $1 AND unlocked_at IS NULL",
    )
    .bind(user_id)
    .execute(&mut *tx)
    .await
    .db_err("Database error")?
    .rows_affected();
    if unlocked > 0 {
        tracing::info!("Account {} unlocked by a password reset", user_id);
    }
    Ok(())
}

/// Middleware: a locked account's still-valid access tokens stop working
/// at once. 401, like an expired token: the client's refresh then fails
/// (the refresh tokens are gone) and it goes to the login page, which
/// explains the lock. One indexed lookup per signed-in request.
pub async fn enforce_account_lock(
    State(state): State<AppState>,
    auth: OptionalAuthUser,
    req: Request,
    next: Next,
) -> Response {
    if let Some(user_id) = auth.user_id {
        let locked = match state.db.acquire().await {
            Ok(mut conn) => is_locked(&mut conn, user_id).await,
            Err(e) => Err(AppError::internal(format!("Database error: {}", e))),
        };
        match locked {
            Ok(false) => {}
            Ok(true) => return AppError::unauthorized("Session ended").into_response(),
            Err(e) => return e.into_response(),
        }
    }
    next.run(req).await
}

#[derive(Debug, Deserialize)]
pub struct ResendRequest {
    pub email: String,
    pub password: String,
}

/// POST /auth/locked/resend-link -- from the login page's lock notice.
pub async fn resend_link(
    State(state): State<AppState>,
    Json(input): Json<ResendRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let user = sqlx::query_as::<_, UserRow>("SELECT * FROM users WHERE LOWER(email) = $1")
        .bind(normalize_email(&input.email))
        .fetch_optional(&state.db)
        .await
        .db_err("Database error")?
        .ok_or_else(|| AppError::bad_request("Invalid email or password"))?;
    verify_password(&user, &input.password)?;

    let mut tx = state.db.begin().await.db_err("Database error")?;
    if !is_locked(&mut tx, user.id).await? {
        return Err(AppError::conflict("This account isn't locked any more; you can log in"));
    }
    let claimed = sqlx::query(
        r#"
        UPDATE account_locks SET links_sent = links_sent + 1, last_link_sent_at = NOW()
        WHERE user_id = $1 AND unlocked_at IS NULL AND links_sent < $2
          AND last_link_sent_at <= NOW() - make_interval(mins => $3)
        "#,
    )
    .bind(user.id)
    .bind(MAX_LINKS_PER_LOCK)
    .bind(RESEND_INTERVAL_MINUTES)
    .execute(&mut *tx)
    .await
    .db_err("Database error")?
    .rows_affected();
    if claimed == 0 {
        return Err(AppError {
            status: StatusCode::TOO_MANY_REQUESTS,
            message: format!(
                "We sent a link recently. You can ask for a new one every {} minutes; if it doesn't arrive, write to us.",
                RESEND_INTERVAL_MINUTES
            ),
        });
    }
    let token = create_reset_token(&mut tx, user.id, LINK_HOURS).await?;
    tx.commit().await.db_err("Database error")?;

    if let Err(e) = state.email.send_account_locked(&user.email, &user.username, &token).await {
        tracing::error!("Lock email resend for user {} failed: {}", user.id, e.0);
    }
    Ok(Json(serde_json::json!({ "message": "We sent you a new link." })))
}

// ── Admin ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct LockRequest {
    /// Why a takeover is suspected. Internal, required.
    pub note: String,
}

pub fn checked_text(text: &str, what: &str) -> Result<String, AppError> {
    let text = text.trim();
    if text.is_empty() {
        return Err(AppError::bad_request(format!("{} is required", what)));
    }
    if text.chars().count() > NOTE_MAX {
        return Err(AppError::bad_request(format!("{} must be under {} characters", what, NOTE_MAX)));
    }
    Ok(text.to_string())
}

/// POST /admin/users/:username/lock (admin only)
pub async fn lock_account(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(username): Path<String>,
    Json(input): Json<LockRequest>,
) -> Result<StatusCode, AppError> {
    require_admin(&state.db, &auth).await?;
    let note = checked_text(&input.note, "A note")?;
    let user_id = sqlx::query_scalar::<_, Uuid>("SELECT id FROM users WHERE LOWER(username) = LOWER($1)")
        .bind(&username)
        .fetch_optional(&state.db)
        .await
        .db_err("Database error")?
        .ok_or_else(|| AppError::not_found("User not found"))?;
    lock_user(&state, user_id, auth.user_id, &note).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Locks an account, ends its sessions and emails the owner a reset link.
/// Also the "lock" decision of an account review (account_review.rs).
pub async fn lock_user(state: &AppState, user_id: Uuid, admin_id: Uuid, note: &str) -> Result<(), AppError> {
    let mut tx = state.db.begin().await.db_err("Database error")?;
    let (email, username) = sqlx::query_as::<_, (String, String)>("SELECT email, username FROM users WHERE id = $1 FOR UPDATE")
        .bind(user_id)
        .fetch_optional(&mut *tx)
        .await
        .db_err("Database error")?
        .ok_or_else(|| AppError::not_found("User not found"))?;
    if user_id == admin_id {
        return Err(AppError::bad_request("You can't lock your own account"));
    }

    let inserted = sqlx::query(
        "INSERT INTO account_locks (user_id, locked_by, note) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
    )
    .bind(user_id)
    .bind(admin_id)
    .bind(note)
    .execute(&mut *tx)
    .await
    .db_err("Database error")?
    .rows_affected();
    if inserted == 0 {
        return Err(AppError::conflict("This account is already locked"));
    }

    sqlx::query("DELETE FROM refresh_tokens WHERE user_id = $1")
        .bind(user_id)
        .execute(&mut *tx)
        .await
        .db_err_ctx("Failed to end sessions", "Database error")?;
    let token = create_reset_token(&mut tx, user_id, LINK_HOURS).await?;
    tx.commit().await.db_err("Database error")?;

    if let Err(e) = state.email.send_account_locked(&email, &username, &token).await {
        tracing::error!("Lock email for user {} failed: {}", user_id, e.0);
    }
    tracing::info!("Account {} locked by admin {} (suspected takeover)", user_id, admin_id);
    Ok(())
}

/// POST /admin/locks/:id/unlock (admin only) -- e.g. once the owner proved
/// who they are by other means.
pub async fn unlock_account(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(lock_id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    require_admin(&state.db, &auth).await?;
    let unlocked = sqlx::query(
        "UPDATE account_locks SET unlocked_at = NOW(), unlocked_via = 'admin', unlocked_by = $2 WHERE id = $1 AND unlocked_at IS NULL",
    )
    .bind(lock_id)
    .bind(auth.user_id)
    .execute(&state.db)
    .await
    .db_err("Database error")?
    .rows_affected();
    if unlocked == 0 {
        return Err(AppError::conflict("This lock isn't active"));
    }
    tracing::info!("Lock {} lifted by admin {}", lock_id, auth.user_id);
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
pub struct AssessmentRequest {
    pub assessment: String,
}

/// PATCH /admin/locks/:id (admin only) -- the incident assessment: what
/// the intruder could see, the risk, and whether it was reported to the
/// data protection authority (Art. 33 GDPR).
pub async fn assess_lock(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(lock_id): Path<Uuid>,
    Json(input): Json<AssessmentRequest>,
) -> Result<StatusCode, AppError> {
    require_admin(&state.db, &auth).await?;
    let assessment = checked_text(&input.assessment, "The assessment")?;
    let updated = sqlx::query(
        "UPDATE account_locks SET assessment = $2, assessed_by = $3, assessed_at = NOW() WHERE id = $1",
    )
    .bind(lock_id)
    .bind(&assessment)
    .bind(auth.user_id)
    .execute(&state.db)
    .await
    .db_err("Database error")?
    .rows_affected();
    if updated == 0 {
        return Err(AppError::not_found("Lock not found"));
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct LockView {
    pub id: Uuid,
    /// None once the account is deleted.
    pub username: Option<String>,
    pub locked_by: Option<String>,
    pub locked_at: DateTime<Utc>,
    pub note: String,
    pub assessment: Option<String>,
    pub assessed_at: Option<DateTime<Utc>>,
    pub links_sent: i32,
    pub last_link_sent_at: DateTime<Utc>,
    pub unlocked_at: Option<DateTime<Utc>>,
    pub unlocked_via: Option<String>,
}

/// GET /admin/locks (admin only) -- the incident log, active first, then
/// newest first.
pub async fn list_locks(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Vec<LockView>>, AppError> {
    require_admin(&state.db, &auth).await?;
    let locks = sqlx::query_as::<_, LockView>(
        r#"
        SELECT l.id, u.username, a.username AS locked_by, l.locked_at, l.note, l.assessment, l.assessed_at,
               l.links_sent, l.last_link_sent_at, l.unlocked_at, l.unlocked_via
        FROM account_locks l
        LEFT JOIN users u ON u.id = l.user_id
        LEFT JOIN users a ON a.id = l.locked_by
        ORDER BY l.unlocked_at IS NOT NULL, l.locked_at DESC
        LIMIT 500
        "#,
    )
    .fetch_all(&state.db)
    .await
    .db_err("Database error")?;
    Ok(Json(locks))
}
