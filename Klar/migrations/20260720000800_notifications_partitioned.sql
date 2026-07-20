-- Notifications, hash-partitioned by user_id.
--
-- Access pattern is always "give me MY notifications" (WHERE user_id =
-- $1) -- hash-by-user_id means that query hits exactly one partition,
-- and it also resolves a real bug in the old schema: the old dedup
-- constraint was UNIQUE(user_id, actor_id, type, post_id, comment_id),
-- but Postgres treats NULL as distinct from NULL in unique constraints --
-- so 'follow' notifications (where post_id/comment_id are both NULL)
-- could duplicate silently on repeated follow/unfollow/follow cycles.
-- Fixed below with an expression index that COALESCEs the nullable
-- columns to a sentinel value so they collapse for dedup purposes.
--
-- (Range-by-created_at was the other option, and fits notifications'
-- "old ones go cold" lifecycle better in the abstract -- but Postgres
-- can't enforce a unique constraint across partitions unless the
-- partition key is part of it, and created_at can't be part of this
-- dedup key without defeating its purpose. Hash-by-user_id sidesteps
-- that entirely, and happens to match the read pattern better too.)
DO $$
BEGIN
    CREATE TYPE notification_type AS ENUM (
        'follow',
        'post_like',
        'comment',
        'comment_like'
    );
EXCEPTION
    WHEN duplicate_object THEN NULL;
END $$;

CREATE TABLE IF NOT EXISTS notifications (
    id         UUID NOT NULL DEFAULT uuid_generate_v7(),
    user_id    UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    actor_id   UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    type       notification_type NOT NULL,
    post_id    UUID REFERENCES posts(id) ON DELETE CASCADE,
    comment_id UUID REFERENCES comments(id) ON DELETE CASCADE,
    is_read    BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (user_id, id)
) PARTITION BY HASH (user_id);

DO $$
BEGIN
    FOR i IN 0..15 LOOP
        EXECUTE format(
            'CREATE TABLE IF NOT EXISTS notifications_p%s PARTITION OF notifications FOR VALUES WITH (MODULUS 16, REMAINDER %s)',
            i, i
        );
    END LOOP;
END $$;

-- Fetching a user's notification list, newest first (per-partition, since
-- user_id is the hash key -- this is a single-partition index scan).
CREATE INDEX IF NOT EXISTS idx_notifications_user_created ON notifications (user_id, created_at DESC);
-- Unread-count / unread-list queries.
CREATE INDEX IF NOT EXISTS idx_notifications_unread ON notifications (user_id) WHERE is_read = FALSE;
-- Dedup index: NULLs coalesced to a sentinel UUID so two 'follow'
-- notifications (post_id/comment_id both NULL) are treated as equal.
CREATE UNIQUE INDEX IF NOT EXISTS idx_notifications_dedup ON notifications (
    user_id, actor_id, type,
    COALESCE(post_id, '00000000-0000-0000-0000-000000000000'),
    COALESCE(comment_id, '00000000-0000-0000-0000-000000000000')
);
