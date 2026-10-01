//! Account standing endpoints (see standing.rs for the model).
//!
//! Users see their own score, the strikes behind it and any suspension on
//! the "Kontostatus" page. Admins see the accounts that reached a threshold
//! or are suspended, each with the suggested next measure, and decide
//! measures and early lifts. Every measure is a decision record with a
//! statement of reasons that the user can object to.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::errors::AppError;
use crate::evidence;
use crate::handlers::auth::AppState;
use crate::handlers::reports::{require_admin, VALID_REASONS};
use crate::moderation::{self, NewMeasure};
use crate::standing::{self, Measure, Suspension, Violation, MAX_SCORE, THRESHOLDS, VIOLATIONS};
use crate::utils::DbResultExt;

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct StrikeView {
    pub id: Uuid,
    pub decision_id: Uuid,
    /// The violation type's id; the labels are filled from the catalog.
    pub violation: String,
    #[sqlx(skip)]
    pub violation_label: &'static str,
    #[sqlx(skip)]
    pub violation_label_de: &'static str,
    pub severity: String,
    /// What the admin gave; `points` is higher when the repeat factor
    /// applied.
    pub base_points: i32,
    pub points: i32,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub reason: String,
    pub target_type: String,
    pub content_excerpt: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Threshold {
    pub score: i32,
    pub measure: &'static str,
}

fn thresholds() -> Vec<Threshold> {
    THRESHOLDS.iter().map(|(score, m)| Threshold { score: *score, measure: m.as_str() }).collect()
}

/// Active strikes, newest first. `delivered_only` hides strikes whose
/// statement is still held back (CSAM), so the user's own view doesn't
/// reveal a decision before an admin has released it.
async fn active_strikes(conn: &mut PgConnection, user_id: Uuid, delivered_only: bool) -> Result<Vec<StrikeView>, AppError> {
    sqlx::query_as::<_, StrikeView>(
        r#"
        SELECT s.id, s.decision_id, s.violation, s.severity, s.base_points, s.points, s.created_at, s.expires_at,
               s.reason::text AS reason, d.target_type::text AS target_type, d.content_excerpt
        FROM account_strikes s JOIN moderation_decisions d ON d.id = s.decision_id
        WHERE s.user_id = $1 AND (s.expires_at IS NULL OR s.expires_at > NOW())
          AND (NOT $2 OR d.delivered_at IS NOT NULL)
        ORDER BY s.created_at DESC
        "#,
    )
    .bind(user_id)
    .bind(delivered_only)
    .fetch_all(&mut *conn)
    .await
    .db_err("Database error")
    .map(|strikes| strikes.into_iter().map(with_labels).collect())
}

fn with_labels(mut s: StrikeView) -> StrikeView {
    if let Some(v) = standing::violation(&s.violation) {
        s.violation_label = v.label;
        s.violation_label_de = v.label_de;
    }
    s
}

fn score_of(strikes: &[StrikeView]) -> i32 {
    strikes.iter().map(|s| s.points).sum::<i32>().min(MAX_SCORE)
}

#[derive(Debug, Serialize)]
pub struct MyStanding {
    pub score: i32,
    pub max_score: i32,
    pub suspension: Option<Suspension>,
    pub strikes: Vec<StrikeView>,
    pub thresholds: Vec<Threshold>,
}

/// GET /users/me/standing
pub async fn my_standing(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<MyStanding>, AppError> {
    let mut conn = state.db.acquire().await.db_err("Database error")?;
    let strikes = active_strikes(&mut conn, auth.user_id, true).await?;
    Ok(Json(MyStanding {
        score: score_of(&strikes),
        max_score: MAX_SCORE,
        suspension: standing::suspension(&mut conn, auth.user_id).await?,
        strikes,
        thresholds: thresholds(),
    }))
}

// ── Admin ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct MeasureView {
    pub id: Uuid,
    pub restriction: String,
    pub reason: String,
    pub suspension_days: Option<i32>,
    pub standing_score: Option<i32>,
    pub created_at: DateTime<Utc>,
    pub lifted_at: Option<DateTime<Utc>>,
    pub superseded: bool,
    pub delivered: bool,
    pub objection_status: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AdminStanding {
    pub user_id: Uuid,
    pub username: String,
    pub score: i32,
    pub max_score: i32,
    pub suspension: Option<Suspension>,
    /// The measure the score suggests next, if any (standing::suggest).
    pub suggestion: Option<&'static str>,
    pub strikes: Vec<StrikeView>,
    pub measures: Vec<MeasureView>,
    pub thresholds: Vec<Threshold>,
}

async fn admin_standing(conn: &mut PgConnection, user_id: Uuid, username: String) -> Result<AdminStanding, AppError> {
    let current = current_standing(conn, user_id).await?;
    Ok(AdminStanding {
        user_id,
        username,
        score: current.score,
        max_score: MAX_SCORE,
        suspension: standing::suspension(conn, user_id).await?,
        suggestion: current.suggestion.map(Measure::as_str),
        strikes: current.strikes,
        measures: current.measures,
        thresholds: thresholds(),
    })
}

/// An account's active strikes and measures, its score, and the measure
/// they suggest next.
struct CurrentStanding {
    strikes: Vec<StrikeView>,
    measures: Vec<MeasureView>,
    score: i32,
    suggestion: Option<Measure>,
}

async fn current_standing(conn: &mut PgConnection, user_id: Uuid) -> Result<CurrentStanding, AppError> {
    let strikes = active_strikes(conn, user_id, false).await?;
    let measures = sqlx::query_as::<_, MeasureView>(
        r#"
        SELECT id, restriction, reason::text AS reason, suspension_days, standing_score, created_at, lifted_at,
               superseded_by IS NOT NULL AS superseded, delivered_at IS NOT NULL AS delivered, objection_status
        FROM moderation_decisions
        WHERE target_type = 'user' AND affected_user_id = $1 AND restriction IN ('warning', 'suspended', 'banned')
        ORDER BY created_at DESC
        "#,
    )
    .bind(user_id)
    .fetch_all(&mut *conn)
    .await
    .db_err("Database error")?;

    let score = score_of(&strikes);
    let last_measure = measures.iter().filter(|m| m.lifted_at.is_none()).map(|m| m.created_at).max();
    let latest_strike = strikes.iter().map(|s| s.created_at).max();
    let strikes_since_last_measure = match (latest_strike, last_measure) {
        (Some(strike), Some(measure)) => strike > measure,
        (Some(_), None) => true,
        (None, _) => false,
    };
    let year_ago = Utc::now() - chrono::Duration::days(365);
    let recently_warned = measures
        .iter()
        .any(|m| m.lifted_at.is_none() && m.created_at > year_ago);

    Ok(CurrentStanding {
        suggestion: standing::suggest(score, strikes_since_last_measure, recently_warned),
        strikes,
        measures,
        score,
    })
}

async fn find_user(conn: &mut PgConnection, username: &str) -> Result<(Uuid, String), AppError> {
    sqlx::query_as::<_, (Uuid, String)>("SELECT id, username FROM users WHERE LOWER(username) = LOWER($1)")
        .bind(username)
        .fetch_optional(&mut *conn)
        .await
        .db_err("Database error")?
        .ok_or_else(|| AppError::not_found("User not found"))
}

/// GET /admin/standing (admin only) -- accounts that reached the warning
/// threshold or are suspended, highest score first.
pub async fn list_standing(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Vec<AdminStanding>>, AppError> {
    require_admin(&state.db, &auth).await?;
    let mut conn = state.db.acquire().await.db_err("Database error")?;

    let users = sqlx::query_as::<_, (Uuid, String)>(
        r#"
        SELECT u.id, u.username
        FROM users u
        LEFT JOIN (
            SELECT user_id, SUM(points) AS points FROM account_strikes
            WHERE expires_at IS NULL OR expires_at > NOW() GROUP BY user_id
        ) s ON s.user_id = u.id
        WHERE COALESCE(s.points, 0) >= $1 OR u.suspended_until > NOW()
        ORDER BY COALESCE(s.points, 0) DESC, u.username
        LIMIT 200
        "#,
    )
    .bind(THRESHOLDS[0].0)
    .fetch_all(&mut *conn)
    .await
    .db_err("Database error")?;

    let mut out = Vec::with_capacity(users.len());
    for (id, username) in users {
        out.push(admin_standing(&mut conn, id, username).await?);
    }
    Ok(Json(out))
}

/// GET /admin/users/:username/standing (admin only)
pub async fn get_user_standing(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(username): Path<String>,
) -> Result<Json<AdminStanding>, AppError> {
    require_admin(&state.db, &auth).await?;
    let mut conn = state.db.acquire().await.db_err("Database error")?;
    let (id, username) = find_user(&mut conn, &username).await?;
    Ok(Json(admin_standing(&mut conn, id, username).await?))
}

#[derive(Debug, Deserialize)]
pub struct MeasureRequest {
    /// "warning", "suspend_7d", "suspend_30d" or "ban".
    pub measure: String,
    /// The report reason the measure mainly rests on; picks the ground
    /// cited in the statement.
    pub reason: String,
    /// Why, in the admin's words; shown to the user in the statement, which
    /// has to name the facts the decision relies on (DSA Art. 17(3)(c)).
    /// Required when the account has no active strike or the measure goes
    /// beyond the suggested one: then the score doesn't explain it.
    pub explanation: Option<String>,
    /// Pending reports on the account that led to the measure. They are
    /// closed with the outcome "account measure", and their reporters told.
    #[serde(default)]
    pub report_ids: Vec<Uuid>,
}

const EXPLANATION_MAX: usize = 1000;

/// POST /admin/users/:username/measures (admin only) -- an admin decides a
/// warning or suspension. Any measure is allowed, not only the suggested
/// one: the suggestion is a guide, the admin weighs the case, and explains
/// it when the score doesn't.
pub async fn apply_measure(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(username): Path<String>,
    Json(input): Json<MeasureRequest>,
) -> Result<Json<AdminStanding>, AppError> {
    require_admin(&state.db, &auth).await?;
    let measure = Measure::parse(&input.measure).ok_or_else(|| AppError::bad_request("Invalid measure"))?;
    if !VALID_REASONS.contains(&input.reason.as_str()) && input.reason != "copyright" {
        return Err(AppError::bad_request("Invalid reason"));
    }
    let explanation = input.explanation.as_deref().map(str::trim).filter(|e| !e.is_empty());
    if explanation.is_some_and(|e| e.chars().count() > EXPLANATION_MAX) {
        return Err(AppError::bad_request(format!("The explanation must be under {} characters", EXPLANATION_MAX)));
    }

    let mut tx = state.db.begin().await.db_err("Database error")?;
    let (user_id, username) = find_user(&mut tx, &username).await?;
    if user_id == auth.user_id {
        return Err(AppError::bad_request("You can't take a measure against your own account"));
    }
    // Locks the user row, so two admins deciding at once serialize.
    sqlx::query("SELECT 1 FROM users WHERE id = $1 FOR UPDATE")
        .bind(user_id)
        .execute(&mut *tx)
        .await
        .db_err("Database error")?;
    let current = current_standing(&mut tx, user_id).await?;
    let has_strikes = !current.strikes.is_empty();
    let beyond_suggestion = current.suggestion.is_none_or(|suggested| measure > suggested);
    if (!has_strikes || beyond_suggestion) && explanation.is_none() {
        return Err(AppError::bad_request(
            "Explain the measure to the user: the account has no active strike or the measure goes beyond the \
             suggested one, so its score doesn't explain it",
        ));
    }
    let report_ids = linked_reports(&mut tx, user_id, &input.report_ids).await?;

    let (_, mut notices) = moderation::record_account_measure(&mut tx, NewMeasure {
        user_id,
        measure,
        reason: &input.reason,
        decided_by: auth.user_id,
        score: current.score,
        has_strikes,
        basis: explanation,
        report_ids: report_ids.clone(),
    })
    .await?;
    if !report_ids.is_empty() {
        notices.extend(
            moderation::close_reports(&mut tx, &report_ids, "actioned", "account_measure", Some(auth.user_id), None).await?,
        );
        // A profile reported for a likely-illegal reason was preserved;
        // the measure decides that too.
        evidence::decide(&mut tx, "user", user_id, auth.user_id, explanation).await?;
    }
    tx.commit().await.db_err("Database error")?;
    notices.send(&state).await;

    let mut conn = state.db.acquire().await.db_err("Database error")?;
    Ok(Json(admin_standing(&mut conn, user_id, username).await?))
}

/// The reports a measure closes: pending reports on this account and
/// nothing else, so a measure can't close unrelated reports by mistake.
async fn linked_reports(tx: &mut PgConnection, user_id: Uuid, ids: &[Uuid]) -> Result<Vec<Uuid>, AppError> {
    let mut ids = ids.to_vec();
    ids.sort();
    ids.dedup();
    if ids.is_empty() {
        return Ok(ids);
    }
    let found = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM reports WHERE id = ANY($1) AND target_type = 'user' AND target_id = $2 AND status = 'pending' FOR UPDATE",
    )
    .bind(&ids)
    .bind(user_id)
    .fetch_all(&mut *tx)
    .await
    .db_err("Database error")?;
    if found.len() != ids.len() {
        return Err(AppError::conflict("Only pending reports on this account can be linked to the measure"));
    }
    Ok(found)
}

/// POST /admin/users/:username/lift-suspension (admin only) -- ends the
/// current suspension early. The decision record keeps who lifted it.
pub async fn lift_suspension(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(username): Path<String>,
) -> Result<StatusCode, AppError> {
    require_admin(&state.db, &auth).await?;
    let mut tx = state.db.begin().await.db_err("Database error")?;
    let (user_id, _) = find_user(&mut tx, &username).await?;
    let notices = moderation::lift_suspension(&mut tx, user_id, auth.user_id)
        .await?
        .ok_or_else(|| AppError::conflict("This account isn't suspended"))?;
    tx.commit().await.db_err("Database error")?;
    notices.send(&state).await;
    Ok(StatusCode::NO_CONTENT)
}

/// GET /admin/violations (admin only) -- the violation catalog, for the
/// classification picker in the report queue.
pub async fn list_violations(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<&'static [Violation]>, AppError> {
    require_admin(&state.db, &auth).await?;
    Ok(Json(VIOLATIONS))
}

#[derive(Debug, Serialize)]
pub struct StrikeDetail {
    #[serde(flatten)]
    pub strike: StrikeView,
    pub username: String,
    /// What was removed, its context and the reports on it
    /// (standing::snapshot_sql).
    pub snapshot: serde_json::Value,
    pub criterion_de: Option<&'static str>,
    /// The reason the reporter picked, when the admin classified it under
    /// another one, and the admin's justification.
    pub reported_reason: Option<String>,
    pub justification: Option<String>,
    pub decided_by: Option<String>,
    pub decided_at: DateTime<Utc>,
    /// The preserved copy with images, for likely-illegal reasons.
    pub evidence_id: Option<Uuid>,
}

/// POST /admin/strikes/:id/open (admin only) -- a strike's snapshot. Every
/// opening is logged (who, when); listing accounts isn't, since it shows
/// only excerpts.
pub async fn open_strike(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(strike_id): Path<Uuid>,
) -> Result<Json<StrikeDetail>, AppError> {
    require_admin(&state.db, &auth).await?;
    let mut tx = state.db.begin().await.db_err("Database error")?;

    let row = sqlx::query_as::<_, (Uuid, String, serde_json::Value, Option<String>, Option<String>, Option<String>, DateTime<Utc>, Option<Uuid>)>(
        r#"
        SELECT s.user_id, u.username, s.snapshot,
               (SELECT r.reason::text FROM reports r WHERE r.id = d.report_ids[1]),
               d.violation_note, du.username, d.created_at,
               (SELECT e.id FROM evidence_records e
                WHERE e.target_type = d.target_type AND e.target_id = d.target_id AND e.purged_at IS NULL
                ORDER BY e.created_at DESC LIMIT 1)
        FROM account_strikes s
        JOIN users u ON u.id = s.user_id
        JOIN moderation_decisions d ON d.id = s.decision_id
        LEFT JOIN users du ON du.id = d.decided_by
        WHERE s.id = $1
        "#,
    )
    .bind(strike_id)
    .fetch_optional(&mut *tx)
    .await
    .db_err("Database error")?
    .ok_or_else(|| AppError::not_found("Strike not found (it may have expired)"))?;
    let (user_id, username, snapshot, reported_reason, justification, decided_by, decided_at, evidence_id) = row;

    let strike = active_strikes(&mut tx, user_id, false)
        .await?
        .into_iter()
        .find(|s| s.id == strike_id)
        .ok_or_else(|| AppError::not_found("Strike not found (it may have expired)"))?;

    sqlx::query("INSERT INTO strike_views (strike_id, user_id, admin_id) VALUES ($1, $2, $3)")
        .bind(strike_id)
        .bind(user_id)
        .bind(auth.user_id)
        .execute(&mut *tx)
        .await
        .db_err("Database error")?;
    tx.commit().await.db_err("Database error")?;
    tracing::info!("Strike {} opened by admin {}", strike_id, auth.user_id);

    let criterion_de = standing::violation(&strike.violation).map(|v| v.criterion_de);
    let reported_reason = reported_reason.filter(|r| *r != strike.reason);
    Ok(Json(StrikeDetail {
        strike,
        username,
        snapshot,
        criterion_de,
        reported_reason,
        justification,
        decided_by,
        decided_at,
        evidence_id,
    }))
}
