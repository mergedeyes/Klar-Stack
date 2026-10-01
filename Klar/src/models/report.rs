//! Content reporting & moderation models.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// POST /reports body.
#[derive(Debug, Deserialize)]
pub struct CreateReportRequest {
    /// "post" | "comment" | "user" | "message"
    pub target_type: String,
    pub target_id: Uuid,
    /// One of the report_reason enum values -- validated against
    /// VALID_REASONS in the handler rather than trusting the DB enum
    /// cast to produce a clean 400 instead of a raw SQL error.
    pub reason: String,
    pub details: Option<String>,
}

/// A single report, as returned to the reporter (on creation) and to
/// admins (in the review queue).
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ReportRow {
    pub id: Uuid,
    /// None once the reporter has deleted their account -- the report
    /// itself is kept (see migration 20260929000200).
    pub reporter_id: Option<Uuid>,
    pub target_type: String,
    pub target_id: Uuid,
    pub reason: String,
    pub details: Option<String>,
    pub status: String,
    pub created_at: DateTime<Utc>,
}
