-- Posts table.
--
-- like_count / comment_count are denormalized (maintained by the app on
-- like/unlike/comment/delete-comment, in the same request as the write
-- that changes them) instead of computed with a correlated COUNT(*)
-- subquery per post on every feed/profile render.
--
-- NOT partitioned, deliberately: post volume grows linearly with content
-- creation (bounded by users x posts-per-user), not combinatorially with
-- engagement the way likes/notifications/feed fan-out do. Partitioning
-- posts would also force every other table's FK into posts(id) to deal
-- with a composite partition key -- not worth it for a table whose growth
-- rate is fundamentally different from the tables that actually need it.
CREATE TABLE IF NOT EXISTS posts (
    id             UUID PRIMARY KEY DEFAULT uuid_generate_v7(),
    user_id        UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    caption        TEXT,
    like_count     BIGINT NOT NULL DEFAULT 0,
    comment_count  BIGINT NOT NULL DEFAULT 0,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    edited_at      TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_posts_user_id ON posts (user_id);
-- Composite index for profile-page keyset pagination (user_id scoped),
-- distinct from the global keyset index used by the discovery feed below.
CREATE INDEX IF NOT EXISTS idx_posts_user_keyset ON posts (user_id, created_at DESC, id DESC);
-- Global keyset pagination index, used by the discovery feed (a full scan
-- across all users, ordered by recency).
CREATE INDEX IF NOT EXISTS idx_posts_keyset_pagination ON posts (created_at DESC, id DESC);
