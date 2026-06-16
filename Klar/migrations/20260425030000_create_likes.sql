-- Likes table
-- Composite primary key: one like per user per post
CREATE TABLE IF NOT EXISTS likes (
    user_id    UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    post_id    UUID NOT NULL REFERENCES posts(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (user_id, post_id)
);

-- "How many likes does this post have?" — queried on every post render
CREATE INDEX IF NOT EXISTS idx_likes_post_id ON likes (post_id);
-- "What posts has this user liked?" — for checking if current user liked a post
CREATE INDEX IF NOT EXISTS idx_likes_user_id ON likes (user_id);
