//! Post models.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// API response — includes author info and edit status
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct PostResponse {
    pub id: Uuid,
    pub user_id: Uuid,
    pub username: String,
    pub avatar_url: Option<String>,
    pub caption: Option<String>,
    pub created_at: DateTime<Utc>,
    pub edited_at: Option<DateTime<Utc>>,
    pub thumb_url: Option<String>,
    pub medium_url: Option<String>,
    pub full_url: Option<String>,
    pub comment_count: i64,
    pub like_count: i64,
    /// "visible" | "flagged" | "hidden" -- see handlers/reports.rs.
    /// "hidden" posts are already excluded from every list/detail query
    /// for non-owners at the SQL level; this field's real job on the
    /// frontend is rendering the "flagged" interstitial. Owners still
    /// see their own hidden/flagged posts (with this field set) so they
    /// know something's under review.
    pub moderation_status: String,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct NewPostResponse {
    pub id: Uuid,
    pub user_id: Uuid,
    pub username: String,
    pub caption: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Request body for creating a post
#[derive(Debug, Deserialize)]
pub struct CreatePostRequest {
    pub caption: Option<String>,
}

/// Request body for editing a post
#[derive(Debug, Deserialize)]
pub struct EditPostRequest {
    pub caption: String,
}

/// Query params for paginated feeds. The cursor is the (created_at, id)
/// of the last post on the previous page; the id breaks ties between posts
/// with the same timestamp, which a timestamp alone would skip at a page
/// boundary.
#[derive(Debug, Deserialize)]
pub struct FeedQuery {
    pub cursor: Option<DateTime<Utc>>,
    pub cursor_id: Option<Uuid>,
    pub limit: Option<i64>,
}

impl FeedQuery {
    /// The keyset to page from, for `(created_at, id) < ($time, $id)`.
    /// Always returns real values instead of NULLs so a single query covers
    /// every page and stays an index range scan:
    /// - no cursor (first page): a far-future time, matching every post;
    /// - a time without an id (clients from before cursor_id existed): the
    ///   nil UUID, which makes the comparison exactly `created_at < time`,
    ///   the old behaviour.
    pub fn keyset(&self) -> (DateTime<Utc>, Uuid) {
        let far_future = DateTime::from_timestamp(253_402_300_799, 0).expect("9999-12-31 is a valid timestamp");
        (
            self.cursor.unwrap_or(far_future),
            self.cursor_id.unwrap_or(Uuid::nil()),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query(cursor: Option<DateTime<Utc>>, cursor_id: Option<Uuid>) -> FeedQuery {
        FeedQuery { cursor, cursor_id, limit: None }
    }

    #[test]
    fn first_page_starts_after_every_post() {
        let (time, id) = query(None, None).keyset();
        assert!(time > Utc::now() + chrono::Duration::days(365 * 1000));
        assert_eq!(id, Uuid::nil());
    }

    #[test]
    fn legacy_time_only_cursor_keeps_strict_time_comparison() {
        let t = Utc::now();
        assert_eq!(query(Some(t), None).keyset(), (t, Uuid::nil()));
    }

    #[test]
    fn full_cursor_is_passed_through() {
        let t = Utc::now();
        let id = Uuid::new_v4();
        assert_eq!(query(Some(t), Some(id)).keyset(), (t, id));
    }
}
