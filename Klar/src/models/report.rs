/// Content reporting & moderation models.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// POST /reports body.
#[derive(Debug, Deserialize)]
pub struct CreateReportRequest {
    /// "post" | "comment" | "user"
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
    pub reporter_id: Uuid,
    pub target_type: String,
    pub target_id: Uuid,
    pub reason: String,
    pub details: Option<String>,
    pub status: String,
    pub created_at: DateTime<Utc>,
}

/// Richer version for the admin queue -- includes the reporter's
/// username and, for post/comment targets, a preview so an admin doesn't
/// have to open another tab to see what's being reported.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct AdminReportRow {
    pub id: Uuid,
    pub reporter_id: Uuid,
    pub reporter_username: String,
    pub target_type: String,
    pub target_id: Uuid,
    pub reason: String,
    pub details: Option<String>,
    pub status: String,
    pub created_at: DateTime<Utc>,
    /// Post caption / comment body, whichever applies. None for
    /// target_type = "user" (nothing textual to preview there).
    pub target_preview: Option<String>,
    /// Post thumbnail (raw storage key) when target_type = "post".
    pub target_thumb_url: Option<String>,
    /// The username being reported, if target_type = "user", or the
    /// author of the reported post/comment otherwise.
    pub target_username: Option<String>,
}
