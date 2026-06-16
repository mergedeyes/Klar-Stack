-- Posts table
CREATE TABLE IF NOT EXISTS posts (
    id          UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id     UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    caption     TEXT,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- We'll query posts by user often (profile page)
CREATE INDEX IF NOT EXISTS idx_posts_user_id ON posts (user_id);
-- Feed queries order by created_at
CREATE INDEX IF NOT EXISTS idx_posts_created_at ON posts (created_at DESC);