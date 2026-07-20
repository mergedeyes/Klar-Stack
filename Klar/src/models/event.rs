/// Interaction event log models — the foundation for future ranking/
/// recommendation work and for export to an analytics store (see the
/// ClickHouse notes). See migrations/20260720001000_post_events_partitioned.sql
/// for the schema and reasoning.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Mirrors the CHECK constraint on post_events.event_type. Kept as a
/// plain string column (not a Postgres ENUM) so adding new event types
/// later doesn't require an ALTER TYPE migration -- just widen the CHECK
/// constraint and this enum together.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    View,
    Like,
    Unlike,
    Comment,
    CommentLike,
    CommentUnlike,
}

impl EventType {
    pub fn as_str(&self) -> &'static str {
        match self {
            EventType::View => "view",
            EventType::Like => "like",
            EventType::Unlike => "unlike",
            EventType::Comment => "comment",
            EventType::CommentLike => "comment_like",
            EventType::CommentUnlike => "comment_unlike",
        }
    }
}

/// Client-reported event, e.g. a post impression from the feed.
/// Interaction events (like/comment/etc.) are recorded server-side
/// directly in the handlers that already know they happened -- this
/// endpoint is only for things the server can't observe on its own,
/// like "this post was actually visible on screen."
#[derive(Debug, Deserialize)]
pub struct RecordEventRequest {
    pub post_id: Uuid,
    pub event_type: String,
}
