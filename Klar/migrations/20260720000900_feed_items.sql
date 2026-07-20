-- feed_items: the fan-out-on-write timeline table.
--
-- Replaces the old live JOIN (follows x posts, computed at request time)
-- for /feed. One row per (follower, post) pair -- populated when the
-- post is created (fanned out to all current followers) and backfilled
-- when a new follow happens (copy the followee's post history in), so a
-- follower always sees the exact same posts they'd have seen under the
-- old live-JOIN model, just pre-computed instead of joined live.
--
-- created_at is copied from the post's own created_at (not the fan-out
-- write time) specifically so the feed stays strictly chronological by
-- original post time -- no reordering, no "ranking", exactly what was
-- asked for.
--
-- Hash-partitioned by user_id (not range-by-time): the query is always
-- "give me user X's feed", so hash-by-user_id keeps a user's entire feed
-- history in one partition (single-partition scan) with no implicit
-- retention window -- unlike range-by-time, which would either force a
-- "only last N months" cutoff or require scatter-gather across many
-- partitions for a single user's feed. Full history was an explicit
-- requirement here, so this is the right tradeoff.
--
-- This intentionally does NOT include the post author's own posts in
-- their own feed_items (matching the previous /feed behavior, where you
-- see who you follow, not yourself -- your own posts live on your
-- profile).
CREATE TABLE IF NOT EXISTS feed_items (
    user_id    UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    post_id    UUID NOT NULL REFERENCES posts(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (user_id, post_id)
) PARTITION BY HASH (user_id);

DO $$
BEGIN
    FOR i IN 0..15 LOOP
        EXECUTE format(
            'CREATE TABLE IF NOT EXISTS feed_items_p%s PARTITION OF feed_items FOR VALUES WITH (MODULUS 16, REMAINDER %s)',
            i, i
        );
    END LOOP;
END $$;

-- The feed read query: WHERE user_id = $1 ORDER BY created_at DESC, post_id DESC
-- (keyset pagination, same pattern as the existing posts/discovery indexes).
CREATE INDEX IF NOT EXISTS idx_feed_items_user_keyset ON feed_items (user_id, created_at DESC, post_id DESC);
