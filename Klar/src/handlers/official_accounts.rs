//! Official accounts: accounts with a verified address at klarsocial.eu,
//! e.g. kontakt@ for the "Klar" profile (see the official_account_renames
//! migration).
//!
//! The sign-up form and the profile settings refuse staff names like
//! "klar" or "support", so nobody can pose as the team. An admin renames
//! an official account straight to such a name here: no reserved-name
//! check and no 14-day cooldown, but the other username rules still apply,
//! including the route names ("me", "search") that would make the profile
//! unreachable. The address must be verified, so whoever registers a
//! klarsocial.eu address without owning the inbox never shows up here.
//! Every rename is logged with the admin and a reason.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::errors::AppError;
use crate::evidence;
use crate::handlers::account_lock::checked_text;
use crate::handlers::auth::AppState;
use crate::handlers::reports::require_admin;
use crate::utils::DbResultExt;
use crate::validation::validate_official_username;

/// The domain of official addresses. Compared against everything after the
/// `@`, so subdomains and lookalikes don't count.
const OFFICIAL_DOMAIN: &str = "klarsocial.eu";

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct OfficialAccount {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct OfficialRename {
    pub id: Uuid,
    pub user_id: Option<Uuid>,
    pub old_username: String,
    pub new_username: String,
    pub reason: String,
    pub renamed_at: DateTime<Utc>,
    /// The admin's current username; None once their account is deleted.
    pub renamed_by: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct OfficialAccounts {
    pub accounts: Vec<OfficialAccount>,
    pub renames: Vec<OfficialRename>,
}

#[derive(Debug, Deserialize)]
pub struct RenameRequest {
    pub username: String,
    pub reason: String,
}

/// GET /admin/official-accounts (admin only) -- the official accounts and
/// every rename, newest first.
pub async fn list(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<OfficialAccounts>, AppError> {
    require_admin(&state.db, &auth).await?;
    let accounts = sqlx::query_as::<_, OfficialAccount>(
        r#"
        SELECT id, username, email, created_at FROM users
        WHERE email_verified AND split_part(LOWER(email), '@', 2) = $1
        ORDER BY created_at
        "#,
    )
    .bind(OFFICIAL_DOMAIN)
    .fetch_all(&state.db)
    .await
    .db_err("Database error")?;
    let renames = sqlx::query_as::<_, OfficialRename>(
        r#"
        SELECT r.id, r.user_id, r.old_username, r.new_username, r.reason, r.renamed_at, a.username AS renamed_by
        FROM official_account_renames r LEFT JOIN users a ON a.id = r.renamed_by
        ORDER BY r.renamed_at DESC
        "#,
    )
    .fetch_all(&state.db)
    .await
    .db_err("Database error")?;
    Ok(Json(OfficialAccounts { accounts, renames }))
}

/// POST /admin/official-accounts/:id/username (admin only) -- renames an
/// official account, staff names allowed.
pub async fn rename(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(user_id): Path<Uuid>,
    Json(input): Json<RenameRequest>,
) -> Result<StatusCode, AppError> {
    require_admin(&state.db, &auth).await?;
    let username = validate_official_username(&input.username)?;
    let reason = checked_text(&input.reason, "A reason")?;

    let mut tx = state.db.begin().await.db_err("Database error")?;
    let old_username = sqlx::query_scalar::<_, String>(
        r#"
        SELECT username FROM users
        WHERE id = $1 AND email_verified AND split_part(LOWER(email), '@', 2) = $2
        FOR UPDATE
        "#,
    )
    .bind(user_id)
    .bind(OFFICIAL_DOMAIN)
    .fetch_optional(&mut *tx)
    .await
    .db_err("Database error")?
    .ok_or_else(|| AppError::not_found("No official account with this id"))?;
    // A different case is a rename too ("klar" -> "Klar"); the same name isn't.
    if old_username == username {
        return Err(AppError::bad_request("The account already has this name"));
    }
    let taken = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM users WHERE LOWER(username) = LOWER($1) AND id != $2)",
    )
    .bind(&username)
    .bind(user_id)
    .fetch_one(&mut *tx)
    .await
    .db_err("Database error")?;
    if taken {
        return Err(AppError::conflict("Username is already taken"));
    }

    sqlx::query("UPDATE users SET username = $1, username_changed_at = NOW() WHERE id = $2")
        .bind(&username)
        .bind(user_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            // Two renames racing for the same name: the unique index decides.
            if e.to_string().contains("duplicate key") {
                AppError::conflict("Username is already taken")
            } else {
                tracing::error!("Official account rename failed: {}", e);
                AppError::internal("Database error")
            }
        })?;
    sqlx::query(
        r#"
        INSERT INTO official_account_renames (user_id, renamed_by, old_username, new_username, reason)
        VALUES ($1, $2, $3, $4, $5)
        "#,
    )
    .bind(user_id)
    .bind(auth.user_id)
    .bind(&old_username)
    .bind(&username)
    .bind(&reason)
    .execute(&mut *tx)
    .await
    .db_err("Database error")?;

    // Like a rename in the profile settings: a profile under a
    // likely-illegal report keeps a version per edit.
    let preserved = evidence::capture(&mut tx, "user", user_id, evidence::Cause::Edited, Some(auth.user_id)).await?;
    tx.commit().await.db_err("Database error")?;
    evidence::finish(&state, preserved, Vec::new()).await;

    tracing::info!("Official account {} renamed from {} to {} by {}", user_id, old_username, username, auth.user_id);
    Ok(StatusCode::NO_CONTENT)
}
