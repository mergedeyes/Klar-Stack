-- post_events comes back, with a purpose this time: ranking the Discovery
-- page by what fits each account. The home feed stays chronological and
-- never reads it. Only interactions the server already handles are logged
-- (likes, unlikes, comments, comment likes); views aren't, since the
-- browser would have to report them, which needs consent (TDDDG § 25).
--
-- Compared with the table dropped in 20261001100200:
-- - user_id is required and goes with the account: an event without its
--   account says nothing about anybody's taste.
-- - No metadata column; nothing needs one.
-- - No 'view' type (see above).
-- - users.personalization_enabled is the opt-out (Art. 21 GDPR): while it
--   is off nothing is logged, and switching it off deletes what was.
-- - Retention: monthly partitions are dropped once all their rows are
--   older than 12 months (post_events_maintain, run by the retention
--   sweep), so an event is kept 12 to 13 months.
--
-- Range-partitioned by month: it's a time-ordered log, read in time ranges
-- (exports to an analytics store, recent signals), and whole months can be
-- dropped without a DELETE.
CREATE TABLE post_events (
    id         UUID NOT NULL DEFAULT uuid_generate_v7(),
    user_id    UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    post_id    UUID NOT NULL REFERENCES posts(id) ON DELETE CASCADE,
    event_type TEXT NOT NULL CHECK (event_type IN ('like', 'unlike', 'comment', 'comment_like', 'comment_unlike')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (id, created_at)
) PARTITION BY RANGE (created_at);

-- Only catches inserts whose month has no partition yet, should the
-- maintenance fall behind; post_events_maintain also applies the
-- retention period to it.
CREATE TABLE post_events_default PARTITION OF post_events DEFAULT;

CREATE INDEX idx_post_events_user ON post_events (user_id, created_at DESC);
CREATE INDEX idx_post_events_post ON post_events (post_id, created_at DESC);

ALTER TABLE users ADD COLUMN personalization_enabled BOOLEAN NOT NULL DEFAULT TRUE;

-- Creates the partitions for this month and the next three, and drops the
-- ones whose rows are all past the retention period. Every replica's
-- retention sweep calls it; the advisory lock keeps two from creating or
-- dropping the same partition at once.
CREATE FUNCTION post_events_maintain(retention INTERVAL) RETURNS void AS $$
DECLARE
    m_start DATE;
    part    TEXT;
BEGIN
    PERFORM pg_advisory_xact_lock(hashtext('post_events_maintain'));

    FOR i IN 0..3 LOOP
        m_start := (date_trunc('month', NOW()) + make_interval(months => i))::date;
        BEGIN
            EXECUTE format(
                'CREATE TABLE IF NOT EXISTS %I PARTITION OF post_events FOR VALUES FROM (%L) TO (%L)',
                'post_events_' || to_char(m_start, 'YYYY_MM'), m_start, m_start + INTERVAL '1 month'
            );
        EXCEPTION WHEN check_violation THEN
            -- Rows of that month already sit in the default partition
            -- (the maintenance stopped for months). They stay there and
            -- still expire through the DELETE below; the drops go on.
            RAISE WARNING 'post_events: partition for % not created, the default partition holds its rows', m_start;
        END;
    END LOOP;

    FOR part IN
        SELECT c.relname FROM pg_inherits inh JOIN pg_class c ON c.oid = inh.inhrelid
        WHERE inh.inhparent = 'post_events'::regclass AND c.relname ~ '^post_events_\d{4}_\d{2}$'
          AND to_date(substring(c.relname FROM 13), 'YYYY_MM') + INTERVAL '1 month' <= NOW() - retention
    LOOP
        EXECUTE format('DROP TABLE %I', part);
    END LOOP;

    DELETE FROM post_events_default WHERE created_at < NOW() - retention;
END;
$$ LANGUAGE plpgsql;

SELECT post_events_maintain(INTERVAL '12 months');
