//! Statements of reasons for users, and the admin side of them (see
//! moderation.rs for how decisions are recorded).
//!
//! Users see the statements about their own content, can object to each
//! one once within six months, and see the outcome of reports they filed.
//! Admins release held-back statements and resolve objections.

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
use crate::handlers::auth::AppState;
use crate::handlers::reports::require_admin;
use crate::moderation;
use crate::standing;
use crate::utils::DbResultExt;

/// How long after a decision the affected user can object (DSA Art. 20(1)
/// asks for at least six months).
const OBJECTION_WINDOW_DAYS: i64 = 183;
const OBJECTION_MIN: usize = 10;
const TEXT_MAX: usize = 2000;

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct DecisionView {
    pub id: Uuid,
    pub target_type: String,
    pub restriction: String,
    pub automated: bool,
    pub reason: String,
    pub ground_type: String,
    pub ground: String,
    pub explanation: String,
    pub content_excerpt: Option<String>,
    /// Length of a temporary suspension (account measures only).
    pub suspension_days: Option<i32>,
    pub created_at: DateTime<Utc>,
    pub lifted_at: Option<DateTime<Utc>>,
    pub superseded: bool,
    pub objection: Option<String>,
    pub objected_at: Option<DateTime<Utc>>,
    pub objection_status: Option<String>,
    pub objection_response: Option<String>,
    pub objection_resolved_at: Option<DateTime<Utc>>,
    /// Whether the user can still object: no objection yet, and within the
    /// window.
    pub can_object: bool,
}

const DECISION_COLUMNS: &str = r#"
    d.id, d.target_type::text AS target_type, d.restriction, d.automated, d.reason::text AS reason,
    d.ground_type, d.ground, d.explanation, d.content_excerpt, d.suspension_days, d.created_at, d.lifted_at,
    d.superseded_by IS NOT NULL AS superseded,
    d.objection, d.objected_at, d.objection_status, d.objection_response, d.objection_resolved_at,
    (d.objection IS NULL AND d.created_at > NOW() - make_interval(days => 183)) AS can_object
"#; // 183 = OBJECTION_WINDOW_DAYS

/// GET /moderation/decisions -- statements about the caller's content,
/// newest first. Held-back statements aren't shown until released.
pub async fn my_decisions(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Vec<DecisionView>>, AppError> {
    let decisions = sqlx::query_as::<_, DecisionView>(&format!(
        "SELECT {DECISION_COLUMNS} FROM moderation_decisions d \
         WHERE d.affected_user_id = $1 AND d.delivered_at IS NOT NULL ORDER BY d.created_at DESC"
    ))
    .bind(auth.user_id)
    .fetch_all(&state.db)
    .await
    .db_err("Database error")?;
    Ok(Json(decisions))
}

/// GET /moderation/decisions/:id -- one statement, for its affected user.
pub async fn get_decision(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(decision_id): Path<Uuid>,
) -> Result<Json<DecisionView>, AppError> {
    sqlx::query_as::<_, DecisionView>(&format!(
        "SELECT {DECISION_COLUMNS} FROM moderation_decisions d \
         WHERE d.id = $1 AND d.affected_user_id = $2 AND d.delivered_at IS NOT NULL"
    ))
    .bind(decision_id)
    .bind(auth.user_id)
    .fetch_optional(&state.db)
    .await
    .db_err("Database error")?
    .map(Json)
    .ok_or_else(|| AppError::not_found("Decision not found"))
}

#[derive(Debug, Deserialize)]
pub struct ObjectionRequest {
    pub text: String,
}

/// POST /moderation/decisions/:id/objection -- the affected user objects,
/// once per decision, within six months.
pub async fn object_to_decision(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(decision_id): Path<Uuid>,
    Json(input): Json<ObjectionRequest>,
) -> Result<Json<DecisionView>, AppError> {
    let text = input.text.trim();
    let len = text.chars().count();
    if len < OBJECTION_MIN {
        return Err(AppError::bad_request(format!(
            "Please explain your objection in at least {} characters", OBJECTION_MIN
        )));
    }
    if len > TEXT_MAX {
        return Err(AppError::bad_request(format!("Objection must be under {} characters", TEXT_MAX)));
    }

    let updated = sqlx::query(
        r#"
        UPDATE moderation_decisions
        SET objection = $3, objected_at = NOW(), objection_status = 'pending'
        WHERE id = $1 AND affected_user_id = $2 AND delivered_at IS NOT NULL
          AND objection IS NULL AND created_at > NOW() - make_interval(days => $4)
        "#,
    )
    .bind(decision_id)
    .bind(auth.user_id)
    .bind(text)
    .bind(OBJECTION_WINDOW_DAYS as i32)
    .execute(&state.db)
    .await
    .db_err("Database error")?
    .rows_affected();
    if updated == 0 {
        return Err(AppError::conflict(
            "This decision can't be objected to (not found, already objected, or older than six months)",
        ));
    }

    tracing::info!("Objection filed against decision {}", decision_id);
    get_decision(State(state), auth, Path(decision_id)).await
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct MyReport {
    pub id: Uuid,
    pub target_type: String,
    pub reason: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub reviewed_at: Option<DateTime<Utc>>,
}

/// GET /moderation/reports -- the reports the caller filed and their
/// outcome (DSA Art. 16(5)). Deliberately nothing about the reported
/// content or person beyond the type.
pub async fn my_reports(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Vec<MyReport>>, AppError> {
    let reports = sqlx::query_as::<_, MyReport>(
        r#"
        SELECT id, target_type::text AS target_type, reason::text AS reason, status::text AS status,
               created_at, reviewed_at
        FROM reports WHERE reporter_id = $1 ORDER BY created_at DESC LIMIT 200
        "#,
    )
    .bind(auth.user_id)
    .fetch_all(&state.db)
    .await
    .db_err("Database error")?;
    Ok(Json(reports))
}

// ── Admin ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct AdminDecision {
    #[sqlx(flatten)]
    #[serde(flatten)]
    pub view: DecisionView,
    pub target_id: Uuid,
    pub affected_username: Option<String>,
    pub delivered_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
pub struct AdminModerationQueue {
    /// Statements held back (CSAM) until an admin releases them.
    pub held: Vec<AdminDecision>,
    /// Objections waiting for a response, oldest first.
    pub objections: Vec<AdminDecision>,
}

fn admin_query(filter: &str, order: &str) -> String {
    format!(
        "SELECT {DECISION_COLUMNS}, d.target_id, u.username AS affected_username, d.delivered_at \
         FROM moderation_decisions d LEFT JOIN users u ON u.id = d.affected_user_id \
         WHERE {filter} ORDER BY {order}"
    )
}

/// GET /admin/moderation (admin only)
pub async fn admin_queue(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<AdminModerationQueue>, AppError> {
    require_admin(&state.db, &auth).await?;

    // Held statements whose author is gone can never be delivered, and one
    // replaced by a later decision (a hide followed by a removal) is covered
    // by that decision's statement, so neither is listed.
    let held = sqlx::query_as::<_, AdminDecision>(&admin_query(
        "d.delivered_at IS NULL AND d.affected_user_id IS NOT NULL AND d.lifted_at IS NULL AND d.superseded_by IS NULL",
        "d.created_at",
    ))
    .fetch_all(&state.db)
    .await
    .db_err("Database error")?;

    let objections = sqlx::query_as::<_, AdminDecision>(&admin_query("d.objection_status = 'pending'", "d.objected_at"))
        .fetch_all(&state.db)
        .await
        .db_err("Database error")?;

    Ok(Json(AdminModerationQueue { held, objections }))
}

/// POST /admin/moderation/decisions/:id/release (admin only) -- sends a
/// held-back statement to its author.
pub async fn release_decision(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(decision_id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    require_admin(&state.db, &auth).await?;

    let mut tx = state.db.begin().await.db_err("Database error")?;
    let (released, notices) = moderation::release_decision(&mut tx, decision_id).await?;
    if !released {
        return Err(AppError::conflict("Already sent, or its author's account no longer exists"));
    }
    tx.commit().await.db_err("Database error")?;
    notices.send(&state).await;

    tracing::info!("Statement {} released by admin {}", decision_id, auth.user_id);
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
pub struct ResolveObjectionRequest {
    /// true: the objection is right, the restriction is lifted where the
    /// content still exists. false: the decision stands.
    pub accept: bool,
    /// Shown to the user; required either way.
    pub response: String,
}

/// POST /admin/moderation/decisions/:id/objection (admin only)
pub async fn resolve_objection(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(decision_id): Path<Uuid>,
    Json(input): Json<ResolveObjectionRequest>,
) -> Result<StatusCode, AppError> {
    require_admin(&state.db, &auth).await?;

    let response = input.response.trim();
    if response.is_empty() {
        return Err(AppError::bad_request("A response to the user is required"));
    }
    if response.chars().count() > TEXT_MAX {
        return Err(AppError::bad_request(format!("Response must be under {} characters", TEXT_MAX)));
    }

    let mut tx = state.db.begin().await.db_err("Database error")?;

    let decision = sqlx::query_as::<_, (String, Uuid, String, Option<Uuid>, bool, Option<Uuid>)>(
        r#"
        UPDATE moderation_decisions
        SET objection_status = $2, objection_response = $3, objection_resolved_at = NOW(), objection_resolved_by = $4,
            lifted_at = CASE WHEN $5 AND restriction != 'removed' THEN COALESCE(lifted_at, NOW()) ELSE lifted_at END
        WHERE id = $1 AND objection_status = 'pending'
        RETURNING target_type::text, target_id, restriction, affected_user_id, superseded_by IS NULL, rights_claim_id
        "#,
    )
    .bind(decision_id)
    .bind(if input.accept { "accepted" } else { "rejected" })
    .bind(response)
    .bind(auth.user_id)
    .bind(input.accept)
    .fetch_optional(&mut *tx)
    .await
    .db_err("Database error")?
    .ok_or_else(|| AppError::not_found("No pending objection for this decision"))?;

    let (target_type, target_id, restriction, affected_user_id, current, rights_claim_id) = decision;

    // An accepted objection against a suspension ends it, if it is still
    // the current one.
    if input.accept && target_type == "user" && moderation::end_suspension(&mut tx, decision_id).await? {
        tracing::info!("Suspension {} ended by accepted objection", decision_id);
    }

    // An accepted objection against a removal takes back its strike.
    if input.accept && restriction == "removed" {
        standing::revoke_strike(&mut tx, decision_id).await?;
    }

    // An accepted objection against a hide or warning makes the content
    // visible again, unless a later decision (a removal) replaced it.
    // Removed content is gone; the response has to say so.
    if input.accept && target_type != "user" && restriction != "removed" && current {
        let table = if target_type == "post" { "posts" } else { "comments" };
        sqlx::query(&format!("UPDATE {table} SET moderation_status = 'visible' WHERE id = $1"))
            .bind(target_id)
            .execute(&mut *tx)
            .await
            .db_err("Database error")?;

        // A post hidden because of a rights claim is back: tell the claimant.
        if let Some(claim_id) = rights_claim_id {
            crate::handlers::rights::mark_restored(&mut tx, &state, claim_id, auth.user_id).await?;
        }
    }

    let notices = match affected_user_id {
        Some(user_id) => moderation::notify_objection_resolved(&mut tx, decision_id, user_id).await?,
        None => moderation::PendingNotices::default(),
    };

    tx.commit().await.db_err("Database error")?;
    notices.send(&state).await;

    tracing::info!(
        "Objection to decision {} {} by admin {}",
        decision_id, if input.accept { "accepted" } else { "rejected" }, auth.user_id
    );
    Ok(StatusCode::NO_CONTENT)
}
