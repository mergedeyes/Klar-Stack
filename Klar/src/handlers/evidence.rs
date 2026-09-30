//! Admin access to preserved evidence (see evidence.rs).
//!
//! Every endpoint that shows content or changes a record takes a written
//! reason and logs it to evidence_events *before* returning anything, so
//! there is no way to read evidence without leaving a trace. The list only
//! shows metadata (type, reasons, dates, status), no content or names.
//!
//! Content and files are fetched with POST so the reason travels in the
//! body rather than the URL (URLs end up in logs), and files are streamed
//! through the backend: the evidence zone has no public URL at all.

use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::Response,
    Json,
};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::errors::AppError;
use crate::evidence::log_event;
use crate::handlers::auth::AppState;
use crate::handlers::reports::require_admin;
use crate::utils::DbResultExt;

const REASON_MAX: usize = 1000;

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct EvidenceSummary {
    pub id: Uuid,
    pub target_type: String,
    pub target_id: Uuid,
    pub reasons: Vec<String>,
    pub created_at: DateTime<Utc>,
    /// Set once the original was deleted, with what deleted it.
    pub content_deleted_at: Option<DateTime<Utc>>,
    pub deletion_trigger: Option<String>,
    pub decision: Option<String>,
    pub decided_at: Option<DateTime<Utc>>,
    pub retain_until: Option<DateTime<Utc>>,
    pub legal_hold: bool,
    pub purged_at: Option<DateTime<Utc>>,
    pub version_count: i64,
    pub file_count: i64,
    /// Still undecided 30 days after it was opened: its report needs a
    /// decision (see evidence::OVERDUE_DAYS).
    pub overdue: bool,
    /// 'required' or 'recommended' when the removal's classification says
    /// it goes to the authorities, and whether a report was recorded.
    pub authority_report: Option<String>,
    pub authority_reported: bool,
}

const SUMMARY_COLUMNS: &str = r#"
    e.id, e.target_type::text AS target_type, e.target_id, e.reasons, e.created_at,
    e.content_deleted_at, e.deletion_trigger,
    e.decision, e.decided_at, e.retain_until, e.legal_hold, e.purged_at,
    (SELECT COUNT(*) FROM evidence_versions v WHERE v.evidence_id = e.id) AS version_count,
    (SELECT COUNT(*) FROM evidence_files f WHERE f.evidence_id = e.id) AS file_count,
    (e.decided_at IS NULL AND e.purged_at IS NULL AND e.created_at < NOW() - INTERVAL '30 days') AS overdue,
    e.authority_report,
    EXISTS (SELECT 1 FROM evidence_events ev WHERE ev.evidence_id = e.id AND ev.action = 'authority_report') AS authority_reported
"#;

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct EvidenceFile {
    pub id: Uuid,
    #[serde(skip)]
    pub version_id: Uuid,
    pub kind: String,
    pub content_type: String,
    pub size_bytes: Option<i64>,
    pub sha256: Option<String>,
    pub copied_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct EvidenceEvent {
    pub id: Uuid,
    pub actor_id: Option<Uuid>,
    /// None for system events, or once the acting account is deleted
    /// (actor_id is kept either way).
    pub actor_username: Option<String>,
    pub action: String,
    pub reason: Option<String>,
    pub details: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
}

/// One captured state of the item, with the files first captured in it.
#[derive(Debug, Serialize)]
pub struct EvidenceVersion {
    pub id: Uuid,
    pub captured_at: DateTime<Utc>,
    /// "reported", "edited" or "deleted".
    pub cause: String,
    pub content: serde_json::Value,
    pub files: Vec<EvidenceFile>,
}

/// A report on the target, read live from the reports table (reports are
/// kept after the content and the reporter's account are gone).
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct EvidenceReport {
    pub id: Uuid,
    pub reason: String,
    pub details: Option<String>,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub reporter_id: Option<Uuid>,
}

#[derive(Debug, Serialize)]
pub struct EvidenceDetail {
    #[serde(flatten)]
    pub summary: EvidenceSummary,
    pub decided_by: Option<Uuid>,
    pub decision_note: Option<String>,
    /// Oldest first; empty once purged.
    pub versions: Vec<EvidenceVersion>,
    pub reports: Vec<EvidenceReport>,
    pub events: Vec<EvidenceEvent>,
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    /// Also list purged records (only their audit trail is left).
    #[serde(default)]
    pub include_purged: bool,
}

#[derive(Debug, Deserialize)]
pub struct AccessReason {
    pub reason: String,
}

#[derive(Debug, Deserialize)]
pub struct HoldRequest {
    pub hold: bool,
    pub reason: String,
}

#[derive(Debug, Deserialize)]
pub struct AuthorityReportRequest {
    /// e.g. "BKA", "jugendschutz.net", "Polizei Berlin".
    pub authority: String,
    pub reported_on: NaiveDate,
    /// The authority's case or reference number, if any.
    pub reference: Option<String>,
    pub note: Option<String>,
}

fn required_reason(reason: &str) -> Result<String, AppError> {
    let reason = reason.trim();
    if reason.is_empty() {
        return Err(AppError::bad_request("A reason is required"));
    }
    if reason.chars().count() > REASON_MAX {
        return Err(AppError::bad_request(format!("Reason must be under {} characters", REASON_MAX)));
    }
    Ok(reason.to_string())
}

fn optional_text(value: Option<String>, field: &str) -> Result<Option<String>, AppError> {
    let value = value.map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
    if value.as_ref().is_some_and(|v| v.chars().count() > REASON_MAX) {
        return Err(AppError::bad_request(format!("{} must be under {} characters", field, REASON_MAX)));
    }
    Ok(value)
}

async fn fetch_summary(state: &AppState, evidence_id: Uuid) -> Result<EvidenceSummary, AppError> {
    sqlx::query_as::<_, EvidenceSummary>(&format!(
        "SELECT {SUMMARY_COLUMNS} FROM evidence_records e WHERE e.id = $1"
    ))
    .bind(evidence_id)
    .fetch_optional(&state.db)
    .await
    .db_err("Database error")?
    .ok_or_else(|| AppError::not_found("Evidence record not found"))
}

/// GET /admin/evidence (admin only) -- newest first, metadata only.
pub async fn list_evidence(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<ListQuery>,
) -> Result<Json<Vec<EvidenceSummary>>, AppError> {
    require_admin(&state.db, &auth).await?;

    let records = sqlx::query_as::<_, EvidenceSummary>(&format!(
        "SELECT {SUMMARY_COLUMNS} FROM evidence_records e \
         WHERE $1 OR e.purged_at IS NULL \
         ORDER BY (e.authority_report = 'required' AND NOT EXISTS (SELECT 1 FROM evidence_events ev \
                   WHERE ev.evidence_id = e.id AND ev.action = 'authority_report')) IS TRUE DESC, \
                  overdue DESC, e.created_at DESC LIMIT 500"
    ))
    .bind(query.include_purged)
    .fetch_all(&state.db)
    .await
    .db_err("Database error")?;

    Ok(Json(records))
}

/// POST /admin/evidence/:id/open (admin only) -- the full record with its
/// content, file list and audit trail. Logged as 'viewed' with the reason.
pub async fn open_evidence(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(evidence_id): Path<Uuid>,
    Json(input): Json<AccessReason>,
) -> Result<Json<EvidenceDetail>, AppError> {
    require_admin(&state.db, &auth).await?;
    let reason = required_reason(&input.reason)?;

    let summary = fetch_summary(&state, evidence_id).await?;

    // Logged before anything is read, so a failed log means no access.
    let mut conn = state.db.acquire().await.db_err("Database error")?;
    log_event(&mut conn, evidence_id, Some(auth.user_id), "viewed", Some(&reason), None).await?;

    let (decided_by, decision_note) = sqlx::query_as::<_, (Option<Uuid>, Option<String>)>(
        "SELECT decided_by, decision_note FROM evidence_records WHERE id = $1",
    )
    .bind(evidence_id)
    .fetch_one(&mut *conn)
    .await
    .db_err("Database error")?;

    let mut files = sqlx::query_as::<_, EvidenceFile>(
        r#"
        SELECT id, version_id, kind, content_type, size_bytes, sha256, copied_at
        FROM evidence_files WHERE evidence_id = $1 ORDER BY kind, sort_order
        "#,
    )
    .bind(evidence_id)
    .fetch_all(&mut *conn)
    .await
    .db_err("Database error")?;

    let versions = sqlx::query_as::<_, (Uuid, DateTime<Utc>, String, serde_json::Value)>(
        "SELECT id, captured_at, cause, content FROM evidence_versions WHERE evidence_id = $1 ORDER BY captured_at, id",
    )
    .bind(evidence_id)
    .fetch_all(&mut *conn)
    .await
    .db_err("Database error")?
    .into_iter()
    .map(|(id, captured_at, cause, content)| {
        let (own, rest): (Vec<_>, Vec<_>) = files.drain(..).partition(|f| f.version_id == id);
        files = rest;
        EvidenceVersion { id, captured_at, cause, content, files: own }
    })
    .collect();

    let reports = sqlx::query_as::<_, EvidenceReport>(
        r#"
        SELECT id, reason::text, details, status::text, created_at, reporter_id
        FROM reports WHERE target_type = $1::report_target_type AND target_id = $2
        ORDER BY created_at
        "#,
    )
    .bind(&summary.target_type)
    .bind(summary.target_id)
    .fetch_all(&mut *conn)
    .await
    .db_err("Database error")?;

    let events = sqlx::query_as::<_, EvidenceEvent>(
        r#"
        SELECT ev.id, ev.actor_id, u.username AS actor_username, ev.action, ev.reason, ev.details, ev.created_at
        FROM evidence_events ev LEFT JOIN users u ON u.id = ev.actor_id
        WHERE ev.evidence_id = $1 ORDER BY ev.created_at
        "#,
    )
    .bind(evidence_id)
    .fetch_all(&mut *conn)
    .await
    .db_err("Database error")?;

    Ok(Json(EvidenceDetail { summary, decided_by, decision_note, versions, reports, events }))
}

/// POST /admin/evidence/:id/files/:file_id (admin only) -- the preserved
/// file itself. Logged as 'file_viewed' with the reason. Before the copy
/// into the evidence zone has succeeded, it's served from the original.
/// Purged records have no file rows left, so they 404 here.
pub async fn get_evidence_file(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((evidence_id, file_id)): Path<(Uuid, Uuid)>,
    Json(input): Json<AccessReason>,
) -> Result<Response, AppError> {
    require_admin(&state.db, &auth).await?;
    let reason = required_reason(&input.reason)?;

    let (source_key, storage_key, content_type, copied) = sqlx::query_as::<_, (String, String, String, bool)>(
        r#"
        SELECT f.source_key, f.storage_key, f.content_type, f.copied_at IS NOT NULL
        FROM evidence_files f
        WHERE f.id = $1 AND f.evidence_id = $2
        "#,
    )
    .bind(file_id)
    .bind(evidence_id)
    .fetch_optional(&state.db)
    .await
    .db_err("Database error")?
    .ok_or_else(|| AppError::not_found("File not found"))?;

    let mut conn = state.db.acquire().await.db_err("Database error")?;
    log_event(
        &mut conn,
        evidence_id,
        Some(auth.user_id),
        "file_viewed",
        Some(&reason),
        Some(json!({ "file_id": file_id })),
    )
    .await?;

    let data = if copied {
        state.evidence.get(&storage_key).await?
    } else {
        state.storage.get(&source_key).await?
    };

    Response::builder()
        .header(header::CONTENT_TYPE, content_type)
        // Nothing may keep a copy: not the browser cache, not a proxy.
        .header(header::CACHE_CONTROL, "no-store")
        .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
        .body(Body::from(data))
        .map_err(|_| AppError::internal("Failed to build response"))
}

/// POST /admin/evidence/:id/hold (admin only) -- sets or lifts a legal hold
/// (an authority request or proceedings). While set, the sweeper won't
/// purge the record even after its retention has passed.
pub async fn set_legal_hold(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(evidence_id): Path<Uuid>,
    Json(input): Json<HoldRequest>,
) -> Result<Json<EvidenceSummary>, AppError> {
    require_admin(&state.db, &auth).await?;
    let reason = required_reason(&input.reason)?;

    let mut tx = state.db.begin().await.db_err("Database error")?;
    let changed = sqlx::query(
        "UPDATE evidence_records SET legal_hold = $2 WHERE id = $1 AND purged_at IS NULL AND legal_hold != $2",
    )
    .bind(evidence_id)
    .bind(input.hold)
    .execute(&mut *tx)
    .await
    .db_err("Database error")?
    .rows_affected();
    if changed == 0 {
        return Err(AppError::conflict("Record not found, already purged, or the hold is already in that state"));
    }

    let action = if input.hold { "hold_set" } else { "hold_lifted" };
    log_event(&mut tx, evidence_id, Some(auth.user_id), action, Some(&reason), None).await?;
    tx.commit().await.db_err("Database error")?;

    tracing::info!("Evidence {}: {} by admin {}", evidence_id, action, auth.user_id);
    Ok(Json(fetch_summary(&state, evidence_id).await?))
}

/// POST /admin/evidence/:id/authority-report (admin only) -- records that,
/// when and to whom the content was reported (e.g. CSAM to the BKA). The
/// report itself happens outside the app.
pub async fn record_authority_report(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(evidence_id): Path<Uuid>,
    Json(input): Json<AuthorityReportRequest>,
) -> Result<StatusCode, AppError> {
    require_admin(&state.db, &auth).await?;

    let authority = required_reason(&input.authority)
        .map_err(|_| AppError::bad_request("The authority is required (under 1000 characters)"))?;
    let reference = optional_text(input.reference, "Reference")?;
    let note = optional_text(input.note, "Note")?;
    // A day of slack: the admin enters a German date, and until 01:00 or
    // 02:00 in Germany the UTC date is still the day before.
    let latest = Utc::now().date_naive() + chrono::Days::new(1);
    if input.reported_on > latest {
        return Err(AppError::bad_request("The report date can't be in the future"));
    }

    // Exists check first: the event table's foreign key would reject an
    // unknown id anyway, but as a raw database error.
    fetch_summary(&state, evidence_id).await?;

    let mut conn = state.db.acquire().await.db_err("Database error")?;
    log_event(
        &mut conn,
        evidence_id,
        Some(auth.user_id),
        "authority_report",
        note.as_deref(),
        Some(json!({ "authority": authority, "reported_on": input.reported_on, "reference": reference })),
    )
    .await?;

    tracing::info!("Evidence {}: authority report recorded by admin {}", evidence_id, auth.user_id);
    Ok(StatusCode::NO_CONTENT)
}
