//! Statements of reasons for users, and the admin side of them (see
//! moderation.rs for how decisions are recorded).
//!
//! Users see the statements about their own content, can object to each
//! one once within six months, and see the outcome of reports they filed.
//! Admins release held-back statements and resolve objections.

use axum::{
    extract::{Path, Query, State},
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
use crate::evidence;
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
    /// The decision that replaced this one (a removal after an automatic
    /// hide), whose statement says what applies now.
    pub superseded_by: Option<Uuid>,
    pub objection: Option<String>,
    pub objected_at: Option<DateTime<Utc>>,
    pub objection_status: Option<String>,
    pub objection_response: Option<String>,
    pub objection_resolved_at: Option<DateTime<Utc>>,
    /// Whether the user can still object: no objection yet, the decision is
    /// still in force (not lifted, not replaced by a later one), and within
    /// the window.
    pub can_object: bool,
    /// What the decision followed: "notice", "own_initiative",
    /// "authority_order", "rights_claim"; None for account measures.
    pub source: Option<String>,
    /// A removal whose content has been deleted for good: an accepted
    /// objection can no longer restore it.
    pub content_purged: bool,
}

const DECISION_COLUMNS: &str = r#"
    d.id, d.target_type::text AS target_type, d.restriction, d.automated, d.reason::text AS reason,
    d.ground_type, d.ground, d.explanation, d.content_excerpt, d.suspension_days, d.created_at, d.lifted_at,
    d.superseded_by IS NOT NULL AS superseded, d.superseded_by,
    d.objection, d.objected_at, d.objection_status, d.objection_response, d.objection_resolved_at,
    (d.objection IS NULL AND d.lifted_at IS NULL AND d.superseded_by IS NULL
     AND d.created_at > NOW() - make_interval(days => 183)) AS can_object,
    d.source, d.content_purged_at IS NOT NULL AS content_purged
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
/// once per decision, within six months, while it is in force: a lifted
/// decision has nothing left to object to, and one replaced by a later
/// decision (a removal after an automatic hide) is objected to through
/// that one.
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
          AND objection IS NULL AND lifted_at IS NULL AND superseded_by IS NULL
          AND created_at > NOW() - make_interval(days => $4)
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
            "This decision can't be objected to (not found, already objected, lifted, replaced by a later decision, or older than six months)",
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
    /// What came of it once reviewed: "removed", "account_measure",
    /// "no_violation", "obsolete" (deleted before review) or "duplicate".
    pub outcome: Option<String>,
    pub created_at: DateTime<Utc>,
    pub reviewed_at: Option<DateTime<Utc>>,
    /// When the reporter asked to have the dismissal checked again.
    pub recheck_requested_at: Option<DateTime<Utc>>,
    /// Dismissed as "no violation" within the last six months and not
    /// re-checked yet: the reporter can ask once.
    pub can_recheck: bool,
}

const MY_REPORT_COLUMNS: &str = r#"
    id, target_type::text AS target_type, reason::text AS reason, status::text AS status,
    outcome, created_at, reviewed_at, recheck_requested_at,
    (status = 'dismissed' AND outcome = 'no_violation' AND recheck_requested_at IS NULL
     AND reviewed_at > NOW() - make_interval(days => 183)) AS can_recheck
"#; // 183 = OBJECTION_WINDOW_DAYS

/// GET /moderation/reports -- the reports the caller filed and their
/// outcome (DSA Art. 16(5)). Deliberately nothing about the reported
/// content or person beyond the type.
pub async fn my_reports(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Vec<MyReport>>, AppError> {
    let reports = sqlx::query_as::<_, MyReport>(&format!(
        "SELECT {MY_REPORT_COLUMNS} FROM reports \
         WHERE reporter_id = $1 AND source = 'user_report' ORDER BY created_at DESC LIMIT 200"
    ))
    .bind(auth.user_id)
    .fetch_all(&state.db)
    .await
    .db_err("Database error")?;
    Ok(Json(reports))
}

#[derive(Debug, Deserialize)]
pub struct RecheckRequest {
    /// Why the reporter thinks it should be looked at again. Optional.
    pub note: Option<String>,
}

/// POST /moderation/reports/:id/recheck -- a reporter asks, once, to have a
/// report checked again that was dismissed as "no violation", within six
/// months (DSA Art. 20(1) opens the complaint system to notifiers too; ⚖️
/// micro and small enterprises are exempt from Art. 20, offered anyway --
/// pending legal review). The report goes back into the queue, marked as a
/// re-check, and is decided there like any other.
pub async fn request_recheck(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(report_id): Path<Uuid>,
    Json(input): Json<RecheckRequest>,
) -> Result<Json<MyReport>, AppError> {
    let note = input.note.as_deref().map(str::trim).filter(|n| !n.is_empty());
    if note.is_some_and(|n| n.chars().count() > TEXT_MAX) {
        return Err(AppError::bad_request(format!("The note must be under {} characters", TEXT_MAX)));
    }

    let mut tx = state.db.begin().await.db_err("Database error")?;
    let reopened = sqlx::query_as::<_, (String, Uuid, String)>(
        r#"
        UPDATE reports
        SET status = 'pending', outcome = NULL, reviewed_at = NULL, reviewed_by = NULL,
            recheck_requested_at = NOW(), recheck_note = $3
        WHERE id = $1 AND reporter_id = $2 AND source = 'user_report'
          AND status = 'dismissed' AND outcome = 'no_violation' AND recheck_requested_at IS NULL
          AND reviewed_at > NOW() - make_interval(days => $4)
        RETURNING target_type::text, target_id, reason::text
        "#,
    )
    .bind(report_id)
    .bind(auth.user_id)
    .bind(note)
    .bind(OBJECTION_WINDOW_DAYS as i32)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| {
        // One pending report per person and item (idx_reports_pending_once).
        if e.as_database_error().and_then(|d| d.constraint()) == Some("idx_reports_pending_once") {
            AppError::conflict("You already have a report on this waiting for review.")
        } else {
            tracing::error!("Re-check request failed: {}", e);
            AppError::internal("Database error")
        }
    })?
    .ok_or_else(|| {
        AppError::conflict("This report can't be checked again (not dismissed, already re-checked, or older than six months)")
    })?;
    let (target_type, target_id, reason) = reopened;

    let exists = sqlx::query_scalar::<_, bool>(match target_type.as_str() {
        "post" => "SELECT EXISTS(SELECT 1 FROM posts WHERE id = $1 AND moderation_status != 'removed')",
        "comment" => "SELECT EXISTS(SELECT 1 FROM comments WHERE id = $1 AND moderation_status != 'removed')",
        "message" => "SELECT EXISTS(SELECT 1 FROM messages WHERE id = $1)",
        _ => "SELECT EXISTS(SELECT 1 FROM users WHERE id = $1)",
    })
    .bind(target_id)
    .fetch_one(&mut *tx)
    .await
    .db_err("Database error")?;
    if !exists {
        return Err(AppError::conflict("The reported content no longer exists"));
    }

    // Preserved again, like a new report: the copy from the first round
    // was purged with the dismissal.
    let preserved = if evidence::preserves(&target_type, &reason) {
        evidence::capture(&mut tx, &target_type, target_id, evidence::Cause::Reported, Some(auth.user_id)).await?
    } else {
        evidence::Preserved::default()
    };
    let report = sqlx::query_as::<_, MyReport>(&format!("SELECT {MY_REPORT_COLUMNS} FROM reports WHERE id = $1"))
        .bind(report_id)
        .fetch_one(&mut *tx)
        .await
        .db_err("Database error")?;
    tx.commit().await.db_err("Database error")?;
    evidence::finish(&state, preserved, Vec::new()).await;

    tracing::info!("Report {} reopened at its reporter's request", report_id);
    Ok(Json(report))
}

/// GET /admin/attention (admin only) -- what is waiting for an admin, for
/// the badges on the admin entries in Settings (the same counts as the
/// daily digest, alerts.rs).
pub async fn admin_attention(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, AppError> {
    require_admin(&state.db, &auth).await?;
    let attention = crate::alerts::attention(&state.db).await?;
    let total = attention.total();
    let mut value = serde_json::to_value(attention).map_err(|_| AppError::internal("Failed to count"))?;
    value["total"] = total.into();
    Ok(Json(value))
}

#[derive(Debug, Deserialize)]
pub struct DecisionLogQuery {
    /// "removed", "hidden", "flagged", "warning", "suspended", "banned".
    pub restriction: Option<String>,
    pub reason: Option<String>,
    /// "notice", "own_initiative", "authority_order", "rights_claim".
    pub source: Option<String>,
    /// Only automatic, or only the team's decisions.
    pub automated: Option<bool>,
    /// The deciding admin's username.
    pub decided_by: Option<String>,
    /// The affected account's username.
    pub affected: Option<String>,
    /// Keyset cursor: the last row of the previous page.
    pub before_time: Option<DateTime<Utc>>,
    pub before_id: Option<Uuid>,
    pub limit: Option<i64>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct LoggedDecision {
    #[sqlx(flatten)]
    #[serde(flatten)]
    pub view: DecisionView,
    pub target_id: Uuid,
    pub affected_username: Option<String>,
    pub decided_by_username: Option<String>,
    pub delivered_at: Option<DateTime<Utc>>,
    pub report_count: i32,
    pub violation_type: Option<String>,
}

/// GET /admin/decisions (admin only) -- every decision, newest first, with
/// filters: who decided what, when and why, for reviewing the team's own
/// work. Read-only; internal justifications stay on the strike and
/// evidence pages, whose opening is logged.
pub async fn decision_log(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<DecisionLogQuery>,
) -> Result<Json<Vec<LoggedDecision>>, AppError> {
    require_admin(&state.db, &auth).await?;
    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    let rows = sqlx::query_as::<_, LoggedDecision>(&format!(
        r#"
        SELECT {DECISION_COLUMNS}, d.target_id, au.username AS affected_username, du.username AS decided_by_username,
               d.delivered_at, cardinality(d.report_ids) AS report_count, d.violation_type
        FROM moderation_decisions d
        LEFT JOIN users au ON au.id = d.affected_user_id
        LEFT JOIN users du ON du.id = d.decided_by
        WHERE ($1::text IS NULL OR d.restriction = $1)
          AND ($2::text IS NULL OR d.reason::text = $2)
          AND ($3::text IS NULL OR d.source = $3)
          AND ($4::bool IS NULL OR d.automated = $4)
          AND ($5::text IS NULL OR LOWER(du.username) = LOWER($5))
          AND ($6::text IS NULL OR LOWER(au.username) = LOWER($6))
          AND ($7::timestamptz IS NULL OR (d.created_at, d.id) < ($7, $8))
        ORDER BY d.created_at DESC, d.id DESC
        LIMIT $9
        "#
    ))
    .bind(q.restriction.as_deref())
    .bind(q.reason.as_deref())
    .bind(q.source.as_deref())
    .bind(q.automated)
    .bind(q.decided_by.as_deref().map(str::trim).filter(|s| !s.is_empty()))
    .bind(q.affected.as_deref().map(str::trim).filter(|s| !s.is_empty()))
    .bind(q.before_time)
    .bind(q.before_id.unwrap_or_else(Uuid::max))
    .bind(limit)
    .fetch_all(&state.db)
    .await
    .db_err("Database error")?;
    Ok(Json(rows))
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
    /// A held-back statement waiting for more than a week.
    pub overdue: bool,
    /// Reports still pending behind an automatic restriction. While there
    /// are any, its objection is answered by deciding them in the queue.
    pub pending_reports: i64,
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
        "SELECT {DECISION_COLUMNS}, d.target_id, u.username AS affected_username, d.delivered_at, \
         (d.delivered_at IS NULL AND d.created_at < NOW() - INTERVAL '7 days') AS overdue, \
         (SELECT COUNT(*) FROM reports r WHERE r.id = ANY(d.report_ids) AND r.status = 'pending' AND d.automated) \
             AS pending_reports \
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
    /// true: the objection is right, the decision is lifted and what it
    /// restricted is restored where that is still possible. false: the
    /// decision stands.
    pub accept: bool,
    /// Shown to the user; required either way.
    pub response: String,
}

/// POST /admin/moderation/decisions/:id/objection (admin only)
///
/// An objection against an automatic restriction whose reports are still
/// pending is answered by deciding those reports in the queue (dismissing
/// accepts it, removing replaces it), so the two can't contradict each
/// other; this refuses it with 409.
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

    let decision = sqlx::query_as::<_, (String, Uuid, String, Option<Uuid>, Option<Uuid>, bool)>(
        r#"
        SELECT target_type::text, target_id, restriction, affected_user_id, rights_claim_id,
               automated AND EXISTS (SELECT 1 FROM reports r WHERE r.id = ANY(report_ids) AND r.status = 'pending')
        FROM moderation_decisions WHERE id = $1 AND objection_status = 'pending'
        FOR UPDATE
        "#,
    )
    .bind(decision_id)
    .fetch_optional(&mut *tx)
    .await
    .db_err("Database error")?
    .ok_or_else(|| AppError::not_found("No pending objection for this decision"))?;
    let (target_type, target_id, restriction, affected_user_id, rights_claim_id, waits_for_reports) = decision;
    if waits_for_reports {
        return Err(AppError::conflict(
            "This automatic restriction waits for a decision on its reports. Decide them in the report queue: \
             dismissing them accepts the objection, removing the content replaces the restriction.",
        ));
    }

    sqlx::query(
        r#"
        UPDATE moderation_decisions
        SET objection_status = $2, objection_response = $3, objection_resolved_at = NOW(), objection_resolved_by = $4,
            lifted_at = CASE WHEN $5 THEN COALESCE(lifted_at, NOW()) ELSE lifted_at END
        WHERE id = $1
        "#,
    )
    .bind(decision_id)
    .bind(if input.accept { "accepted" } else { "rejected" })
    .bind(response)
    .bind(auth.user_id)
    .bind(input.accept)
    .execute(&mut *tx)
    .await
    .db_err("Database error")?;

    let mut unused_avatar = None;
    if input.accept {
        // A suspension ends, if it is still the current one.
        if target_type == "user" && moderation::end_suspension(&mut tx, decision_id).await? {
            tracing::info!("Suspension {} ended by accepted objection", decision_id);
        }
        // What was removed from a profile goes back.
        if target_type == "user" && restriction == "removed" {
            if let Some(user_id) = affected_user_id {
                unused_avatar = crate::handlers::profile_moderation::restore(&mut tx, decision_id, user_id).await?;
            }
        }
        if restriction == "removed" {
            // The strike goes, and the evidence kept because of the removal
            // is purged like a dismissed report's (unless a hold or a report
            // to the authorities keeps it).
            standing::revoke_strike(&mut tx, decision_id).await?;
            evidence::dismiss_after_objection(&mut tx, &target_type, target_id, auth.user_id).await?;
        }
        // What the decision restricted becomes visible again, unless
        // another restriction still in force keeps it hidden. Removed
        // content is restored as long as it hasn't been deleted for good.
        let status = moderation::refresh_status(&mut tx, &target_type, target_id).await?;
        // A post hidden because of a rights claim is back: tell the claimant.
        if let Some(claim_id) = rights_claim_id {
            let visible = matches!(status.as_ref().map(|(_, new)| new.as_str()), Some("visible" | "flagged"));
            crate::handlers::rights::mark_restored(&mut tx, &state, claim_id, auth.user_id, visible).await?;
        }
    }

    let notices = match affected_user_id {
        Some(user_id) => moderation::notify_objection_resolved(&mut tx, decision_id, user_id).await?,
        None => moderation::PendingNotices::default(),
    };

    tx.commit().await.db_err("Database error")?;
    notices.send(&state).await;
    if let Some(key) = unused_avatar {
        evidence::release_media(&state, &key).await;
    }

    tracing::info!(
        "Objection to decision {} {} by admin {}",
        decision_id, if input.accept { "accepted" } else { "rejected" }, auth.user_id
    );
    Ok(StatusCode::NO_CONTENT)
}
