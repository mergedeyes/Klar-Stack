-- post_events: append-only interaction/event log.
--
-- This is the foundation for any future ranking/recommendation work, and
-- for exporting to an analytics store (see the ClickHouse notes in this
-- rollout's summary). The core idea: you cannot retroactively reconstruct
-- user behavior you never recorded, so this starts capturing it now, long
-- before there's an actual ranking algorithm to feed.
--
-- Deliberately NOT deduplicated -- a user viewing the same post twice is
-- two valid rows, not a conflict, unlike likes/notifications which have
-- real "did this already happen" semantics.
--
-- Range-partitioned by created_at (monthly): this table is genuinely a
-- time-ordered log, primarily read via time-range scans (batch export to
-- an analytics store, or "what happened this week") rather than
-- "give me everything for user X" -- the opposite access pattern from
-- feed_items/notifications, so the opposite partitioning strategy is
-- correct here. It also gives you the archival benefit for free: old
-- monthly partitions can be dropped or moved to cold storage once
-- they've been exported, without touching current data.
CREATE TABLE IF NOT EXISTS post_events (
    id         UUID NOT NULL DEFAULT uuid_generate_v7(),
    user_id    UUID REFERENCES users(id) ON DELETE SET NULL,
    post_id    UUID NOT NULL REFERENCES posts(id) ON DELETE CASCADE,
    event_type TEXT NOT NULL CHECK (event_type IN ('view', 'like', 'unlike', 'comment', 'comment_like', 'comment_unlike')),
    metadata   JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (id, created_at)
) PARTITION BY RANGE (created_at);

-- Helper to create one monthly partition on demand. A scheduled job
-- (cron, or a periodic Rust task) should call this a month or two ahead
-- of need -- e.g. `SELECT create_post_events_partition(2027, 1);` -- so
-- inserts never rely on the DEFAULT partition below except as a safety
-- net for when that job falls behind.
CREATE OR REPLACE FUNCTION create_post_events_partition(p_year INT, p_month INT) RETURNS void AS $$
DECLARE
    start_date DATE := make_date(p_year, p_month, 1);
    end_date   DATE := start_date + INTERVAL '1 month';
    part_name  TEXT := format('post_events_%s', to_char(start_date, 'YYYY_MM'));
BEGIN
    EXECUTE format(
        'CREATE TABLE IF NOT EXISTS %I PARTITION OF post_events FOR VALUES FROM (%L) TO (%L)',
        part_name, start_date, end_date
    );
END;
$$ LANGUAGE plpgsql;

-- Pre-create a runway of partitions (12 months back, 12 forward from
-- today) so this doesn't need manual attention immediately. Whatever is
-- outside this window falls into the DEFAULT partition rather than
-- failing the insert -- a safety net, not a substitute for the
-- maintenance job.
DO $$
DECLARE
    base DATE := date_trunc('month', NOW())::date;
    i INT;
BEGIN
    FOR i IN -12..12 LOOP
        PERFORM create_post_events_partition(
            extract(year FROM base + (i || ' months')::interval)::int,
            extract(month FROM base + (i || ' months')::interval)::int
        );
    END LOOP;
END $$;

CREATE TABLE IF NOT EXISTS post_events_default PARTITION OF post_events DEFAULT;

CREATE INDEX IF NOT EXISTS idx_post_events_post_id ON post_events (post_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_post_events_user_id ON post_events (user_id, created_at DESC);
