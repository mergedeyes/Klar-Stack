//! Rights claims: formal copyright (and similar) notices from rightsholders.
//!
//! Unlike an ordinary report, a claim can come from anyone, with or without
//! a Klar account (DSA Art. 16(1)), through the public form at /rights. It
//! carries the claimant's name and email, the post, the work and why they
//! hold the rights, and a good-faith statement (Art. 16(2)). The claimant
//! gets a private status link (a random token, stored hashed) and follows
//! the claim there: an admin triages it, may ask for evidence, and accepts
//! or declines it.
//!
//! Accepting hides the post rather than deleting it, and sends the uploader
//! a statement of reasons (moderation.rs). If the uploader's objection
//! succeeds, the post is restored and the claim marked 'restored' (see
//! handlers/moderation.rs). Claims are deleted three years after they were
//! decided, together with their audit trail.
//!
//! ⚖️ Pending legal review: whether Klar falls under the UrhDaG, the
//! retention period, and whether the uploader should learn the claimant's
//! identity (currently only the claimed work is shared).

use std::time::Duration;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::{generate_refresh_token, hash_refresh_token, AuthUser, OptionalAuthUser};
use crate::errors::AppError;
use crate::handlers::auth::AppState;
use crate::handlers::reports::{require_admin, rotate_post_media_keys};
use crate::moderation::{self, NewDecision, Restriction};
use crate::utils::DbResultExt;
use crate::validation::{required_text, validate_new_email};

const SHORT_MAX: usize = 200;
const URL_MAX: usize = 500;
const TEXT_MAX: usize = 4000;
/// Claims are kept this long after their decision, for disputes about it
/// (the regular limitation period, § 195 BGB). ⚖️ Pending legal review.
const RETENTION_DAYS: i32 = 3 * 365;

/// Finds the post a URL points to: "…/posts/<uuid>" in a full link or a
/// bare path. Posts are the only claimable content for now.
fn post_id_from_url(url: &str) -> Option<Uuid> {
    let rest = url.split("/posts/").nth(1)?;
    let id: String = rest.chars().take_while(|c| c.is_ascii_hexdigit() || *c == '-').collect();
    Uuid::parse_str(&id).ok()
}

fn optional_text(value: Option<String>, field: &str, max: usize) -> Result<Option<String>, AppError> {
    let value = value.map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
    if let Some(v) = &value {
        if v.chars().count() > max {
            return Err(AppError::bad_request(format!("{} must be {} characters or less", field, max)));
        }
    }
    Ok(value)
}

async fn log_event(
    conn: &mut sqlx::PgConnection,
    claim_id: Uuid,
    actor_id: Option<Uuid>,
    action: &str,
    note: Option<&str>,
) -> Result<(), AppError> {
    sqlx::query("INSERT INTO rights_claim_events (claim_id, actor_id, action, note) VALUES ($1, $2, $3, $4)")
        .bind(claim_id)
        .bind(actor_id)
        .bind(action)
        .bind(note)
        .execute(&mut *conn)
        .await
        .db_err_ctx("Failed to log rights claim event", "Database error")?;
    Ok(())
}

/// Sends a claimant email in the background; the claim is stored either
/// way, and its status page shows the same information.
fn email_claimant(state: &AppState, to: String, claim_id: Uuid, subject: &'static str, message: String) {
    let state = state.clone();
    tokio::spawn(async move {
        if let Err(e) = state.email.send_rights_claim_update(&to, claim_id, subject, &message).await {
            tracing::error!("Rights claim {} email failed: {}", claim_id, e.0);
        }
    });
}

// ── Public ────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateClaimRequest {
    pub claim_type: String,
    pub claimant_name: String,
    pub claimant_email: String,
    pub claimant_organization: Option<String>,
    pub represented_party: Option<String>,
    pub content_url: String,
    pub work_description: String,
    pub ownership_basis: String,
    pub original_url: Option<String>,
    pub good_faith: bool,
    /// Honeypot: hidden in the form, so only bots fill it in.
    #[serde(default)]
    pub website: String,
}

#[derive(Debug, Serialize)]
pub struct CreateClaimResponse {
    pub id: Uuid,
    /// The claimant's key to the status page; also sent by email. Only its
    /// hash is stored.
    pub token: String,
}

/// POST /rights-claims (public, strict rate limit)
pub async fn create_claim(
    State(state): State<AppState>,
    auth: OptionalAuthUser,
    Json(input): Json<CreateClaimRequest>,
) -> Result<(StatusCode, Json<CreateClaimResponse>), AppError> {
    if !input.website.is_empty() {
        // Looks accepted to the bot; nothing is stored.
        tracing::info!("Rights claim honeypot triggered");
        return Ok((StatusCode::CREATED, Json(CreateClaimResponse { id: Uuid::nil(), token: String::new() })));
    }
    if !matches!(input.claim_type.as_str(), "copyright" | "trademark" | "other") {
        return Err(AppError::bad_request("Invalid claim type"));
    }
    if !input.good_faith {
        return Err(AppError::bad_request("The good-faith statement is required"));
    }
    let name = required_text(&input.claimant_name, "Name", SHORT_MAX)?;
    let email = validate_new_email(&input.claimant_email)?;
    let organization = optional_text(input.claimant_organization, "Organization", SHORT_MAX)?;
    let represented = optional_text(input.represented_party, "Represented party", SHORT_MAX)?;
    let content_url = required_text(&input.content_url, "Link to the post", URL_MAX)?;
    let work = required_text(&input.work_description, "Description of the work", TEXT_MAX)?;
    let basis = required_text(&input.ownership_basis, "Basis of your rights", TEXT_MAX)?;
    let original_url = optional_text(input.original_url, "Link to the original", URL_MAX)?;

    let post_id = post_id_from_url(content_url)
        .ok_or_else(|| AppError::bad_request("The link must point to a post on Klar (…/posts/…)"))?;
    let exists = sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM posts WHERE id = $1)")
        .bind(post_id)
        .fetch_one(&state.db)
        .await
        .db_err("Database error")?;
    if !exists {
        return Err(AppError::not_found("That post doesn't exist (any more)"));
    }

    let token = generate_refresh_token();
    let mut tx = state.db.begin().await.db_err("Database error")?;
    let id = sqlx::query_scalar::<_, Uuid>(
        r#"
        INSERT INTO rights_claims
            (claim_type, claimant_name, claimant_email, claimant_organization, represented_party,
             claimant_user_id, content_url, target_type, target_id, work_description, ownership_basis,
             original_url, good_faith, status_token_hash)
        VALUES ($1, $2, $3, $4, $5, $6, $7, 'post', $8, $9, $10, $11, TRUE, $12)
        RETURNING id
        "#,
    )
    .bind(&input.claim_type)
    .bind(name)
    .bind(&email)
    .bind(organization)
    .bind(represented)
    .bind(auth.user_id)
    .bind(content_url)
    .bind(post_id)
    .bind(work)
    .bind(basis)
    .bind(original_url)
    .bind(hash_refresh_token(&token))
    .fetch_one(&mut *tx)
    .await
    .db_err_ctx("Failed to store rights claim", "Database error")?;
    log_event(&mut tx, id, None, "submitted", None).await?;
    tx.commit().await.db_err("Database error")?;

    // Confirmation of receipt (Art. 16(4)) with the status link.
    {
        let state = state.clone();
        let token = token.clone();
        tokio::spawn(async move {
            if let Err(e) = state.email.send_rights_claim_received(&email, id, &token).await {
                tracing::error!("Rights claim {} confirmation email failed: {}", id, e.0);
            }
        });
    }

    tracing::info!("Rights claim {} submitted for post {}", id, post_id);
    Ok((StatusCode::CREATED, Json(CreateClaimResponse { id, token })))
}

#[derive(Debug, Deserialize)]
pub struct TokenRequest {
    pub token: String,
    /// Only for /respond.
    pub response: Option<String>,
}

/// What the claimant sees on their status page.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ClaimStatus {
    pub id: Uuid,
    pub claim_type: String,
    pub content_url: String,
    pub work_description: String,
    pub status: String,
    pub evidence_request: Option<String>,
    pub claimant_response: Option<String>,
    pub decision_reason: Option<String>,
    pub created_at: DateTime<Utc>,
    pub decided_at: Option<DateTime<Utc>>,
}

async fn claim_for_token(state: &AppState, claim_id: Uuid, token: &str) -> Result<ClaimStatus, AppError> {
    sqlx::query_as::<_, ClaimStatus>(
        r#"
        SELECT id, claim_type, content_url, work_description, status, evidence_request,
               claimant_response, decision_reason, created_at, decided_at
        FROM rights_claims WHERE id = $1 AND status_token_hash = $2
        "#,
    )
    .bind(claim_id)
    .bind(hash_refresh_token(token.trim()))
    .fetch_optional(&state.db)
    .await
    .db_err("Database error")?
    // Same answer for "no such claim" and "wrong token".
    .ok_or_else(|| AppError::not_found("Claim not found"))
}

/// POST /rights-claims/:id/status (public; the token is the key). POST so
/// the token travels in the body, not in a URL that ends up in logs.
pub async fn claim_status(
    State(state): State<AppState>,
    Path(claim_id): Path<Uuid>,
    Json(input): Json<TokenRequest>,
) -> Result<Json<ClaimStatus>, AppError> {
    Ok(Json(claim_for_token(&state, claim_id, &input.token).await?))
}

/// POST /rights-claims/:id/respond (public) -- the claimant's answer to an
/// evidence request. Puts the claim back in the admin queue.
pub async fn respond_to_claim(
    State(state): State<AppState>,
    Path(claim_id): Path<Uuid>,
    Json(input): Json<TokenRequest>,
) -> Result<Json<ClaimStatus>, AppError> {
    claim_for_token(&state, claim_id, &input.token).await?;
    let response = required_text(input.response.as_deref().unwrap_or(""), "Response", TEXT_MAX)?;

    let mut tx = state.db.begin().await.db_err("Database error")?;
    let updated = sqlx::query(
        "UPDATE rights_claims SET claimant_response = $2, status = 'triaged' WHERE id = $1 AND status = 'evidence_requested'",
    )
    .bind(claim_id)
    .bind(response)
    .execute(&mut *tx)
    .await
    .db_err("Database error")?
    .rows_affected();
    if updated == 0 {
        return Err(AppError::conflict("No evidence was requested for this claim"));
    }
    log_event(&mut tx, claim_id, None, "claimant_responded", None).await?;
    tx.commit().await.db_err("Database error")?;

    Ok(Json(claim_for_token(&state, claim_id, &input.token).await?))
}

// ── Admin ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct AdminClaim {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub claim_type: String,
    pub claimant_name: String,
    pub claimant_email: String,
    pub claimant_organization: Option<String>,
    pub represented_party: Option<String>,
    pub claimant_username: Option<String>,
    pub content_url: String,
    pub target_id: Uuid,
    /// Whether the post still exists, and whose it is.
    pub target_exists: bool,
    pub target_username: Option<String>,
    pub target_caption: Option<String>,
    pub work_description: String,
    pub ownership_basis: String,
    pub original_url: Option<String>,
    pub status: String,
    pub evidence_request: Option<String>,
    pub claimant_response: Option<String>,
    pub decision_reason: Option<String>,
    pub decided_at: Option<DateTime<Utc>>,
    pub decision_id: Option<Uuid>,
    /// Open for more than 30 days without a decision.
    pub overdue: bool,
}

#[derive(Debug, Deserialize)]
pub struct ClaimsQuery {
    /// "open" (default) or "all".
    pub filter: Option<String>,
}

/// GET /admin/rights-claims (admin only) -- open claims oldest first, so
/// nothing waits unnoticed.
pub async fn list_claims(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<ClaimsQuery>,
) -> Result<Json<Vec<AdminClaim>>, AppError> {
    require_admin(&state.db, &auth).await?;
    let all = query.filter.as_deref() == Some("all");

    let claims = sqlx::query_as::<_, AdminClaim>(
        r#"
        SELECT c.id, c.created_at, c.claim_type, c.claimant_name, c.claimant_email, c.claimant_organization,
               c.represented_party, cu.username AS claimant_username, c.content_url, c.target_id,
               p.id IS NOT NULL AS target_exists, pu.username AS target_username, p.caption AS target_caption,
               c.work_description, c.ownership_basis, c.original_url, c.status, c.evidence_request,
               c.claimant_response, c.decision_reason, c.decided_at, c.decision_id,
               (c.decided_at IS NULL AND c.created_at < NOW() - INTERVAL '30 days') AS overdue
        FROM rights_claims c
        LEFT JOIN users cu ON cu.id = c.claimant_user_id
        LEFT JOIN posts p ON p.id = c.target_id
        LEFT JOIN users pu ON pu.id = p.user_id
        WHERE $1 OR c.status IN ('submitted', 'triaged', 'evidence_requested')
        ORDER BY CASE WHEN $1 THEN NULL ELSE c.created_at END ASC NULLS LAST, c.created_at DESC
        LIMIT 500
        "#,
    )
    .bind(all)
    .fetch_all(&state.db)
    .await
    .db_err("Database error")?;
    Ok(Json(claims))
}

/// Locks an open claim and returns (claimant_email, target_id, work).
async fn lock_open_claim(tx: &mut sqlx::PgConnection, claim_id: Uuid) -> Result<(String, Uuid, String), AppError> {
    sqlx::query_as::<_, (String, Uuid, String)>(
        r#"
        SELECT claimant_email, target_id, work_description FROM rights_claims
        WHERE id = $1 AND status IN ('submitted', 'triaged', 'evidence_requested')
        FOR UPDATE
        "#,
    )
    .bind(claim_id)
    .fetch_optional(&mut *tx)
    .await
    .db_err("Database error")?
    .ok_or_else(|| AppError::not_found("No open claim with this id"))
}

/// POST /admin/rights-claims/:id/triage (admin only) -- "someone is on it".
pub async fn triage_claim(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(claim_id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    require_admin(&state.db, &auth).await?;
    let mut tx = state.db.begin().await.db_err("Database error")?;
    let updated = sqlx::query("UPDATE rights_claims SET status = 'triaged' WHERE id = $1 AND status = 'submitted'")
        .bind(claim_id)
        .execute(&mut *tx)
        .await
        .db_err("Database error")?
        .rows_affected();
    if updated == 0 {
        return Err(AppError::conflict("Only a newly submitted claim can be triaged"));
    }
    log_event(&mut tx, claim_id, Some(auth.user_id), "triaged", None).await?;
    tx.commit().await.db_err("Database error")?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
pub struct MessageRequest {
    pub message: String,
}

/// POST /admin/rights-claims/:id/request-evidence (admin only) -- asks the
/// claimant for more (e.g. proof of authorship). They answer on the
/// status page.
pub async fn request_evidence(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(claim_id): Path<Uuid>,
    Json(input): Json<MessageRequest>,
) -> Result<StatusCode, AppError> {
    require_admin(&state.db, &auth).await?;
    let message = required_text(&input.message, "Message", TEXT_MAX)?;

    let mut tx = state.db.begin().await.db_err("Database error")?;
    let (email, _, _) = lock_open_claim(&mut tx, claim_id).await?;
    sqlx::query(
        "UPDATE rights_claims SET status = 'evidence_requested', evidence_request = $2, claimant_response = NULL WHERE id = $1",
    )
    .bind(claim_id)
    .bind(message)
    .execute(&mut *tx)
    .await
    .db_err("Database error")?;
    log_event(&mut tx, claim_id, Some(auth.user_id), "evidence_requested", Some(message)).await?;
    tx.commit().await.db_err("Database error")?;

    email_claimant(&state, email, claim_id, "Rueckfrage zu deiner Rechte-Meldung bei Klar", message.to_string());
    Ok(StatusCode::NO_CONTENT)
}

/// POST /admin/rights-claims/:id/accept (admin only) -- hides the post
/// (restorable) and sends its author a statement of reasons.
pub async fn accept_claim(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(claim_id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    require_admin(&state.db, &auth).await?;

    let mut tx = state.db.begin().await.db_err("Database error")?;
    let (email, post_id, work) = lock_open_claim(&mut tx, claim_id).await?;

    let newly_hidden = match sqlx::query_scalar::<_, String>(
        "SELECT moderation_status::text FROM posts WHERE id = $1 FOR UPDATE",
    )
    .bind(post_id)
    .fetch_optional(&mut *tx)
    .await
    .db_err("Database error")?
    {
        None => return Err(AppError::conflict("The post no longer exists -- decline the claim with that reason instead")),
        Some(status) => status != "hidden",
    };
    if newly_hidden {
        sqlx::query("UPDATE posts SET moderation_status = 'hidden' WHERE id = $1")
            .bind(post_id)
            .execute(&mut *tx)
            .await
            .db_err("Database error")?;
    }

    let notices = moderation::record_decision(&mut tx, NewDecision {
        target_type: "post",
        target_id: post_id,
        restriction: Restriction::Hidden,
        automated: false,
        reason: "copyright",
        decided_by: Some(auth.user_id),
        report_id: None,
        rights_claim_id: Some(claim_id),
        detail: Some(format!("Betroffenes Werk laut Meldung: {}", work)),
        // Repeat infringers are a separate, still open question; a hide
        // after a rights claim doesn't count towards the standing yet.
        classification: None,
    })
    .await?;

    sqlx::query(
        r#"
        UPDATE rights_claims SET status = 'accepted', decided_at = NOW(), decided_by = $2,
            decision_id = (SELECT id FROM moderation_decisions WHERE rights_claim_id = $1 ORDER BY created_at DESC LIMIT 1)
        WHERE id = $1
        "#,
    )
    .bind(claim_id)
    .bind(auth.user_id)
    .execute(&mut *tx)
    .await
    .db_err("Database error")?;
    log_event(&mut tx, claim_id, Some(auth.user_id), "accepted", None).await?;
    tx.commit().await.db_err("Database error")?;

    notices.send(&state).await;
    email_claimant(
        &state,
        email,
        claim_id,
        "Deine Rechte-Meldung bei Klar wurde angenommen",
        "Wir haben deine Meldung geprueft und den Beitrag ausgeblendet. Die Person, die ihn veroeffentlicht hat, \
         wurde informiert und kann widersprechen; ueber das Ergebnis informieren wir dich."
            .to_string(),
    );
    // Hiding only stops the API handing out links; moving the files ends
    // links that were already copied.
    if newly_hidden {
        let state = state.clone();
        tokio::spawn(async move {
            if let Err(e) = rotate_post_media_keys(&state, post_id).await {
                tracing::error!("Media key rotation for claimed post {} failed: {}", post_id, e.message);
            }
        });
    }

    tracing::info!("Rights claim {} accepted by admin {}", claim_id, auth.user_id);
    Ok(StatusCode::NO_CONTENT)
}

/// POST /admin/rights-claims/:id/decline (admin only) -- with a reason the
/// claimant sees.
pub async fn decline_claim(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(claim_id): Path<Uuid>,
    Json(input): Json<MessageRequest>,
) -> Result<StatusCode, AppError> {
    require_admin(&state.db, &auth).await?;
    let reason = required_text(&input.message, "Reason", TEXT_MAX)?;

    let mut tx = state.db.begin().await.db_err("Database error")?;
    let (email, _, _) = lock_open_claim(&mut tx, claim_id).await?;
    sqlx::query(
        "UPDATE rights_claims SET status = 'declined', decision_reason = $2, decided_at = NOW(), decided_by = $3 WHERE id = $1",
    )
    .bind(claim_id)
    .bind(reason)
    .bind(auth.user_id)
    .execute(&mut *tx)
    .await
    .db_err("Database error")?;
    log_event(&mut tx, claim_id, Some(auth.user_id), "declined", Some(reason)).await?;
    tx.commit().await.db_err("Database error")?;

    email_claimant(&state, email, claim_id, "Deine Rechte-Meldung bei Klar wurde abgelehnt", reason.to_string());
    tracing::info!("Rights claim {} declined by admin {}", claim_id, auth.user_id);
    Ok(StatusCode::NO_CONTENT)
}

/// Called when the uploader's objection to a claim's decision is accepted
/// (the post is visible again): marks the claim restored and tells the
/// claimant. Runs inside the resolving transaction.
pub async fn mark_restored(
    tx: &mut sqlx::PgConnection,
    state: &AppState,
    claim_id: Uuid,
    admin_id: Uuid,
) -> Result<(), AppError> {
    let email = sqlx::query_scalar::<_, String>(
        "UPDATE rights_claims SET status = 'restored' WHERE id = $1 AND status = 'accepted' RETURNING claimant_email",
    )
    .bind(claim_id)
    .fetch_optional(&mut *tx)
    .await
    .db_err("Database error")?;
    if let Some(email) = email {
        log_event(tx, claim_id, Some(admin_id), "restored", None).await?;
        email_claimant(
            state,
            email,
            claim_id,
            "Update zu deiner Rechte-Meldung bei Klar",
            "Die Person, die den Beitrag veroeffentlicht hat, hat erfolgreich widersprochen. Der Beitrag ist \
             wieder sichtbar. Dir stehen weiterhin der Rechtsweg und die aussergerichtliche Streitbeilegung offen."
                .to_string(),
        );
    }
    Ok(())
}

/// Deletes claims decided more than RETENTION_DAYS ago (with their audit
/// trail), once a day.
pub fn spawn_cleanup(db: sqlx::PgPool) {
    tokio::spawn(async move {
        loop {
            match delete_expired(&db).await {
                Ok(n) if n > 0 => tracing::info!("Deleted {} rights claims past retention", n),
                Ok(_) => {}
                Err(e) => tracing::error!("Rights claim cleanup failed: {}", e),
            }
            tokio::time::sleep(Duration::from_secs(24 * 60 * 60)).await;
        }
    });
}

/// One cleanup pass; returns how many rows went.
pub(crate) async fn delete_expired(db: &sqlx::PgPool) -> Result<u64, sqlx::Error> {
    sqlx::query("DELETE FROM rights_claims WHERE decided_at < NOW() - make_interval(days => $1)")
        .bind(RETENTION_DAYS)
        .execute(db)
        .await
        .map(|r| r.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_post_ids_in_links() {
        let id = Uuid::new_v4();
        assert_eq!(post_id_from_url(&format!("https://www.klarsocial.eu/posts/{id}")), Some(id));
        assert_eq!(post_id_from_url(&format!("/posts/{id}?x=1")), Some(id));
        assert_eq!(post_id_from_url(&format!("klarsocial.de/posts/{id}/")), Some(id));
        assert_eq!(post_id_from_url("https://www.klarsocial.eu/users/someone"), None);
        assert_eq!(post_id_from_url("https://www.klarsocial.eu/posts/not-a-uuid"), None);
    }
}
