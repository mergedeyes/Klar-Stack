-- Settings -> Your activity lists an account's own likes and comments,
-- newest first and paged by (created_at, id) (handlers/activity.rs). These
-- replace the plain user_id indices, which they cover as a prefix, so a
-- page is an index range scan instead of sorting everything the account
-- ever liked or wrote.
CREATE INDEX IF NOT EXISTS idx_likes_user_created ON likes (user_id, created_at DESC, post_id DESC);
DROP INDEX IF EXISTS idx_likes_user_id;

CREATE INDEX IF NOT EXISTS idx_comments_user_created ON comments (user_id, created_at DESC, id DESC);
DROP INDEX IF EXISTS idx_comments_user_id;
