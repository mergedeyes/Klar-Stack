/// Like models.

use serde::Serialize;

/// Response after liking/unliking
#[derive(Debug, Serialize)]
pub struct LikeResponse {
    pub liked: bool,
    pub like_count: i64,
}
