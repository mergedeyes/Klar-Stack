-- Likes, hash-partitioned by post_id.
--
-- This is the table most likely to hit real scale first: every (user,
-- post) like pair, across every post ever created. Hash partitioning
-- (rather than range-by-time) is the right fit here because:
--   1. The natural uniqueness key (post_id, user_id) doesn't need
--      created_at in it -- a range partition would force that, which
--      would break the simple "does this like already exist" check.
--   2. There's no natural "cold data" to archive -- a like on a post
--      from a year ago is exactly as relevant as one from a minute ago,
--      as long as the post still exists. Range-by-time buys you nothing
--      here; even distribution across partitions is what actually helps
--      (smaller indexes per partition, spread-out write load).
--
-- 16 partitions is a starting point. Note the real limitation of hash
-- partitioning: changing the partition count later requires detaching
-- and rebuilding partitions (no simple "add one more" like you get with
-- range partitioning) -- so this number is worth deliberately sizing for
-- where you expect to be in a couple of years, not just today.
CREATE TABLE IF NOT EXISTS likes (
    user_id    UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    post_id    UUID NOT NULL REFERENCES posts(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (post_id, user_id)
) PARTITION BY HASH (post_id);

DO $$
BEGIN
    FOR i IN 0..15 LOOP
        EXECUTE format(
            'CREATE TABLE IF NOT EXISTS likes_p%s PARTITION OF likes FOR VALUES WITH (MODULUS 16, REMAINDER %s)',
            i, i
        );
    END LOOP;
END $$;

-- Postgres automatically creates a local index per partition for the
-- partitioned PK; this extra index serves "who liked this post" queries
-- ordered by recency (e.g. a future "recent likers" list).
CREATE INDEX IF NOT EXISTS idx_likes_post_id_created ON likes (post_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_likes_user_id ON likes (user_id);
