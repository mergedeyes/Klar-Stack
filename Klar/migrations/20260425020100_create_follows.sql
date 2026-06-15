-- Follows table — the social graph
-- Composite primary key prevents duplicate follows
CREATE TABLE follows (
    follower_id  UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    following_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (follower_id, following_id),
    -- Can't follow yourself
    CONSTRAINT no_self_follow CHECK (follower_id != following_id)
);

-- "Who am I following?" (for feed generation)
CREATE INDEX idx_follows_follower ON follows (follower_id);
-- "Who follows me?" (for follower list)
CREATE INDEX idx_follows_following ON follows (following_id);