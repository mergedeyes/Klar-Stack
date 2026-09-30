//! Notices about changes to the Terms of Service and the privacy policy
//! (see the legal_updates migration).
//!
//! A notice is a file in klar-web/legal-updates (which documents changed and
//! a short summary in plain language), written in the pull request that
//! changes the page; CI refuses a legal-page change without one. The
//! frontend deploy sends the files here once the new page is live
//! (`publish_from_deploy`); an admin can also publish one by hand. Every account created before that sees it in
//! the app on its next visit; a Terms change has to be accepted there (the
//! acceptance is recorded as proof), a privacy change only acknowledged.
//! Verified addresses also get an email, sent one by one in the background;
//! each send is claimed in legal_update_emails first, so the hourly sweep
//! resumes an interrupted run without mailing anyone twice.

use std::time::Duration;

use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    Json,
};
use sha2::{Digest, Sha256};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::errors::AppError;
use crate::handlers::auth::AppState;
use crate::handlers::reports::require_admin;
use crate::utils::DbResultExt;

const SUMMARY_MAX: usize = 2000;
/// Pause between emails, to stay well inside the email provider's limits.
const SEND_PAUSE: Duration = Duration::from_millis(200);

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct PendingUpdate {
    pub id: Uuid,
    pub published_at: DateTime<Utc>,
    pub documents: Vec<String>,
    pub summary: String,
    pub requires_acceptance: bool,
}

/// GET /legal-updates/pending -- notices the caller hasn't seen or accepted
/// yet, oldest first. Accounts created after a notice accepted the version
/// it announces at sign-up, so they never see it.
pub async fn pending(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Vec<PendingUpdate>>, AppError> {
    let updates = sqlx::query_as::<_, PendingUpdate>(
        r#"
        SELECT l.id, l.published_at, l.documents, l.summary, l.requires_acceptance
        FROM legal_updates l, users u
        WHERE u.id = $1 AND l.published_at > u.created_at
          AND NOT EXISTS (SELECT 1 FROM legal_update_acks a WHERE a.update_id = l.id AND a.user_id = $1)
        ORDER BY l.published_at
        "#,
    )
    .bind(auth.user_id)
    .fetch_all(&state.db)
    .await
    .db_err("Database error")?;
    Ok(Json(updates))
}

/// POST /legal-updates/:id/acknowledge -- "Verstanden" for a privacy
/// notice, "Akzeptieren" for a Terms change (which also moves the
/// account's terms_accepted_at).
pub async fn acknowledge(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(update_id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let mut tx = state.db.begin().await.db_err("Database error")?;
    let requires_acceptance = sqlx::query_scalar::<_, bool>("SELECT requires_acceptance FROM legal_updates WHERE id = $1")
        .bind(update_id)
        .fetch_optional(&mut *tx)
        .await
        .db_err("Database error")?
        .ok_or_else(|| AppError::not_found("Notice not found"))?;

    let inserted = sqlx::query(
        "INSERT INTO legal_update_acks (update_id, user_id, accepted) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
    )
    .bind(update_id)
    .bind(auth.user_id)
    .bind(requires_acceptance)
    .execute(&mut *tx)
    .await
    .db_err("Database error")?
    .rows_affected();

    if inserted > 0 && requires_acceptance {
        sqlx::query("UPDATE users SET terms_accepted_at = NOW() WHERE id = $1")
            .bind(auth.user_id)
            .execute(&mut *tx)
            .await
            .db_err("Database error")?;
    }
    tx.commit().await.db_err("Database error")?;
    Ok(StatusCode::NO_CONTENT)
}

// ── Admin ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct PublishRequest {
    /// "terms" and/or "privacy".
    pub documents: Vec<String>,
    pub summary: String,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct AdminUpdate {
    pub id: Uuid,
    /// The file in klar-web/legal-updates, or None if published by hand.
    pub source_key: Option<String>,
    pub published_at: DateTime<Utc>,
    pub documents: Vec<String>,
    pub summary: String,
    pub requires_acceptance: bool,
    pub emails_sent: i64,
    pub emails_finished_at: Option<DateTime<Utc>>,
    /// Accounts that existed when it was published (and still exist).
    pub audience: i64,
    pub acknowledged: i64,
}

/// Checks and stores a notice. Returns None when `source_key` was already
/// published (the deploy sends every file on every run).
async fn insert_update(
    state: &AppState,
    documents: Vec<String>,
    summary: &str,
    published_by: Option<Uuid>,
    source_key: Option<&str>,
) -> Result<Option<Uuid>, AppError> {
    let mut documents: Vec<String> = documents.into_iter().filter(|d| d == "terms" || d == "privacy").collect();
    documents.sort();
    documents.dedup();
    if documents.is_empty() {
        return Err(AppError::bad_request("Pick the Terms, the privacy policy, or both"));
    }
    let summary = summary.trim();
    if summary.chars().count() < 20 {
        return Err(AppError::bad_request("Summarise what changed in at least 20 characters"));
    }
    if summary.chars().count() > SUMMARY_MAX {
        return Err(AppError::bad_request(format!("The summary must be under {} characters", SUMMARY_MAX)));
    }
    let requires_acceptance = documents.iter().any(|d| d == "terms");

    let id = sqlx::query_scalar::<_, Uuid>(
        r#"
        INSERT INTO legal_updates (published_by, source_key, documents, summary, requires_acceptance)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (source_key) DO NOTHING
        RETURNING id
        "#,
    )
    .bind(published_by)
    .bind(source_key)
    .bind(&documents)
    .bind(summary)
    .bind(requires_acceptance)
    .fetch_optional(&state.db)
    .await
    .db_err("Database error")?;

    if let Some(id) = id {
        tracing::info!("Legal update {} ({:?}, {:?}) published", id, documents, source_key);
        let state = state.clone();
        tokio::spawn(async move { send_emails(&state).await });
    }
    Ok(id)
}

/// POST /admin/legal-updates (admin only) -- publishes a notice by hand.
/// Normally notices come from klar-web/legal-updates on deploy (below).
pub async fn publish(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<PublishRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    require_admin(&state.db, &auth).await?;
    let id = insert_update(&state, input.documents, &input.summary, Some(auth.user_id), None)
        .await?
        .expect("no source key, so never a duplicate");
    Ok((StatusCode::CREATED, Json(serde_json::json!({ "id": id }))))
}

#[derive(Debug, Deserialize)]
pub struct DeployRequest {
    /// The file name in klar-web/legal-updates, e.g. "2026-10-01-sicherer-ort.md".
    pub key: String,
    pub documents: Vec<String>,
    pub summary: String,
}

/// Whether `key` looks like a notice file name: lowercase letters, digits
/// and dashes, ending in ".md".
fn valid_key(key: &str) -> bool {
    key.len() <= 100
        && key.strip_suffix(".md").is_some_and(|stem| {
            !stem.is_empty() && stem.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        })
}

/// Compares the bearer token with LEGAL_UPDATES_TOKEN through their
/// SHA-256 digests, so the comparison's timing says nothing about the
/// secret. Unset or empty: the endpoint is off.
fn deploy_token_ok(headers: &HeaderMap) -> bool {
    let Ok(expected) = std::env::var("LEGAL_UPDATES_TOKEN") else { return false };
    if expected.is_empty() {
        return false;
    }
    let Some(given) = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
    else {
        return false;
    };
    Sha256::digest(given.as_bytes()) == Sha256::digest(expected.as_bytes())
}

/// POST /internal/legal-updates -- the frontend deploy publishes the
/// notice files once the new legal pages are live. Authenticated with
/// LEGAL_UPDATES_TOKEN, not an account. Idempotent: a file already
/// published answers 200 with created: false.
pub async fn publish_from_deploy(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<DeployRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    if !deploy_token_ok(&headers) {
        return Err(AppError::unauthorized("Invalid token"));
    }
    if !valid_key(&input.key) {
        return Err(AppError::bad_request("Invalid key"));
    }
    match insert_update(&state, input.documents, &input.summary, None, Some(&input.key)).await? {
        Some(id) => Ok((StatusCode::CREATED, Json(serde_json::json!({ "id": id, "created": true })))),
        None => Ok((StatusCode::OK, Json(serde_json::json!({ "created": false })))),
    }
}

/// GET /admin/legal-updates (admin only) -- notices with their progress.
pub async fn list(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Vec<AdminUpdate>>, AppError> {
    require_admin(&state.db, &auth).await?;
    let updates = sqlx::query_as::<_, AdminUpdate>(
        r#"
        SELECT l.id, l.source_key, l.published_at, l.documents, l.summary, l.requires_acceptance, l.emails_finished_at,
               (SELECT COUNT(*) FROM legal_update_emails e WHERE e.update_id = l.id) AS emails_sent,
               (SELECT COUNT(*) FROM users u WHERE u.created_at < l.published_at) AS audience,
               (SELECT COUNT(*) FROM legal_update_acks a WHERE a.update_id = l.id) AS acknowledged
        FROM legal_updates l ORDER BY l.published_at DESC LIMIT 50
        "#,
    )
    .fetch_all(&state.db)
    .await
    .db_err("Database error")?;
    Ok(Json(updates))
}

/// Emails every verified address that existed before a notice and hasn't
/// been mailed about it yet. Each send is claimed first (ON CONFLICT: a
/// replica or an earlier run got there), so nobody gets it twice.
pub(crate) async fn send_emails(state: &AppState) {
    let open = match sqlx::query_as::<_, (Uuid, Vec<String>, String, bool)>(
        "SELECT id, documents, summary, requires_acceptance FROM legal_updates WHERE emails_finished_at IS NULL ORDER BY published_at",
    )
    .fetch_all(&state.db)
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            tracing::error!("Legal update emails: listing notices failed: {}", e);
            return;
        }
    };

    for (update_id, documents, summary, requires_acceptance) in open {
        loop {
            let next = sqlx::query_as::<_, (Uuid, String)>(
                r#"
                INSERT INTO legal_update_emails (update_id, user_id)
                SELECT $1, u.id FROM users u, legal_updates l
                WHERE l.id = $1 AND u.created_at < l.published_at AND u.email_verified
                  AND NOT EXISTS (SELECT 1 FROM legal_update_emails e WHERE e.update_id = $1 AND e.user_id = u.id)
                ORDER BY u.created_at LIMIT 1
                ON CONFLICT DO NOTHING
                RETURNING user_id, (SELECT email FROM users WHERE id = user_id)
                "#,
            )
            .bind(update_id)
            .fetch_optional(&state.db)
            .await;
            match next {
                Ok(Some((user_id, email))) => {
                    if let Err(e) = state.email.send_legal_update(&email, &documents, &summary, requires_acceptance).await {
                        tracing::error!("Legal update {} email to user {} failed: {}", update_id, user_id, e.0);
                    }
                    tokio::time::sleep(SEND_PAUSE).await;
                }
                Ok(None) => {
                    // No row back: either nobody is left, or another run
                    // claimed the same person first. Only the first means done.
                    let left = sqlx::query_scalar::<_, bool>(
                        r#"
                        SELECT EXISTS (
                            SELECT 1 FROM users u, legal_updates l
                            WHERE l.id = $1 AND u.created_at < l.published_at AND u.email_verified
                              AND NOT EXISTS (SELECT 1 FROM legal_update_emails e WHERE e.update_id = $1 AND e.user_id = u.id)
                        )
                        "#,
                    )
                    .bind(update_id)
                    .fetch_one(&state.db)
                    .await;
                    match left {
                        Ok(true) => continue,
                        Ok(false) => {
                            let _ = sqlx::query(
                                "UPDATE legal_updates SET emails_finished_at = NOW() WHERE id = $1 AND emails_finished_at IS NULL",
                            )
                            .bind(update_id)
                            .execute(&state.db)
                            .await;
                            tracing::info!("Legal update {}: all emails sent", update_id);
                            break;
                        }
                        Err(e) => {
                            tracing::error!("Legal update {} emails: {}", update_id, e);
                            break;
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("Legal update {} emails: {}", update_id, e);
                    break;
                }
            }
        }
    }
}

/// Resumes interrupted email runs once an hour (e.g. after a restart).
pub fn spawn_sweeper(state: AppState) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(60 * 60)).await;
            send_emails(&state).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notice_keys_are_plain_file_names() {
        assert!(valid_key("2026-10-01-sicherer-ort.md"));
        assert!(!valid_key("../secrets.md"));
        assert!(!valid_key("Notice.md"));
        assert!(!valid_key(".md"));
        assert!(!valid_key("2026-10-01.txt"));
    }
}
