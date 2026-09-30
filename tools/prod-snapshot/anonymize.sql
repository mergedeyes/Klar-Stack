-- Anonymizes a freshly restored production dump. Runs in one transaction
-- (psql --single-transaction): if anything fails, nothing is committed and
-- snapshot.sh drops the import database.
--
-- Keeps: every id, foreign key, timestamp, counter, enum and flag, so the
-- structure (who follows whom, how many posts, moderation states, ...)
-- matches production exactly. Replaces: everything a person typed or that
-- identifies them.
--
-- Every account's password becomes "klar-dev-password" (argon2id, same
-- parameters as the backend), so you can log in locally as any user.
--
-- The guard at the end aborts the transaction if the schema has a
-- text-like column that isn't listed in reviewed_columns below. When a
-- migration adds one, decide here whether it holds personal data
-- (anonymize it above) or not, then add it to the list.

\set ON_ERROR_STOP on

-- ── Users ────────────────────────────────────────────────────────────────────
WITH numbered AS (
    SELECT id, row_number() OVER (ORDER BY created_at, id) AS n FROM users
)
UPDATE users u SET
    username      = 'user_' || n.n,
    email         = 'user_' || n.n || '@example.invalid',
    display_name  = CASE WHEN u.display_name IS NULL THEN NULL ELSE 'User ' || n.n END,
    bio           = CASE WHEN u.bio IS NULL THEN NULL ELSE 'Bio of user ' || n.n END,
    avatar_url    = NULL,
    password_hash = '$argon2id$v=19$m=19456,t=2,p=1$CBuy+VqyNkwZcDS+/2I8nA$y2c59sZG8I/Fy0hQxvq7XXyQUx58ACLCSCmWQc8RG9g'
FROM numbered n
WHERE u.id = n.id;

-- ── Content ──────────────────────────────────────────────────────────────────
-- Replaced with neutral text of the same length (capped), so length-based
-- checks and UI layout behave like production without any real content.
UPDATE posts    SET caption = left(repeat('Lorem ipsum ', 200), char_length(caption)) WHERE caption IS NOT NULL;
UPDATE comments SET body    = left(repeat('Lorem ipsum ', 200), char_length(body));
UPDATE messages SET body    = left(repeat('Lorem ipsum ', 200), char_length(body));
UPDATE reports  SET details     = CASE WHEN details     IS NULL THEN NULL ELSE 'Report details' END,
                    review_note = CASE WHEN review_note IS NULL THEN NULL ELSE 'Review note' END;
UPDATE moderation_decisions SET
    content_excerpt    = CASE WHEN content_excerpt    IS NULL THEN NULL ELSE 'Content excerpt' END,
    objection          = CASE WHEN objection          IS NULL THEN NULL ELSE 'Objection text' END,
    objection_response = CASE WHEN objection_response IS NULL THEN NULL ELSE 'Objection response' END;

-- Uploaded images are personal data; point every asset at one placeholder
-- key (files aren't downloaded anyway, this just avoids real CDN URLs).
UPDATE media_assets SET
    original_key = 'anon/placeholder.webp',
    thumb_key    = 'anon/placeholder.webp',
    medium_key   = 'anon/placeholder.webp',
    full_key     = 'anon/placeholder.webp';

UPDATE post_events SET metadata = NULL WHERE metadata IS NOT NULL;

-- ── Secrets ──────────────────────────────────────────────────────────────────
TRUNCATE refresh_tokens, email_tokens;

-- ── Evidence ─────────────────────────────────────────────────────────────────
-- Preserved evidence of possibly illegal content (evidence.rs) never leaves
-- production: not anonymized, dropped entirely. Its files live in their own
-- storage zone, which the snapshot doesn't touch.
TRUNCATE evidence_files, evidence_versions, evidence_events, evidence_records;

-- ── Guard: every text-like column must be reviewed ───────────────────────────
DO $$
DECLARE
    reviewed_columns text[] := ARRAY[
        -- anonymized above
        'users.username', 'users.email', 'users.display_name', 'users.bio',
        'users.avatar_url', 'users.password_hash',
        'posts.caption', 'comments.body', 'messages.body',
        'reports.details', 'reports.review_note',
        'moderation_decisions.content_excerpt', 'moderation_decisions.objection',
        'moderation_decisions.objection_response',
        'media_assets.original_key', 'media_assets.thumb_key',
        'media_assets.medium_key', 'media_assets.full_key',
        'post_events.metadata',
        -- emptied above
        'refresh_tokens.token_hash', 'refresh_tokens.device_info',
        'email_tokens.token', 'email_tokens.token_type',
        'evidence_records.target_type', 'evidence_records.deletion_trigger',
        'evidence_records.decision', 'evidence_records.decision_note',
        'evidence_versions.cause', 'evidence_versions.content',
        'evidence_files.kind', 'evidence_files.source_key', 'evidence_files.storage_key',
        'evidence_files.content_type', 'evidence_files.sha256',
        'evidence_events.action', 'evidence_events.reason', 'evidence_events.details',
        -- not personal data (fixed vocabularies / bookkeeping)
        'comments.moderation_status', 'posts.moderation_status',
        'notifications.type', 'post_events.event_type',
        'reports.reason', 'reports.status', 'reports.target_type',
        -- statement templates from moderation.rs, not user input
        'moderation_decisions.target_type', 'moderation_decisions.restriction',
        'moderation_decisions.reason', 'moderation_decisions.ground_type',
        'moderation_decisions.ground', 'moderation_decisions.explanation',
        'moderation_decisions.objection_status',
        'message_reactions.emoji',
        '_sqlx_migrations.description', '_sqlx_migrations.checksum'
    ];
    unreviewed text;
BEGIN
    SELECT string_agg(c.relname || '.' || a.attname, ', ' ORDER BY 1)
    INTO unreviewed
    FROM pg_attribute a
    JOIN pg_class c      ON c.oid = a.attrelid
    JOIN pg_namespace ns ON ns.oid = c.relnamespace
    JOIN pg_type t       ON t.oid = a.atttypid
    WHERE ns.nspname = 'public'
      AND c.relkind IN ('r', 'p')          -- tables and partitioned parents
      AND NOT c.relispartition             -- partitions inherit the parent's columns
      AND a.attnum > 0 AND NOT a.attisdropped
      AND (t.typcategory IN ('S', 'E')      -- strings, enums
           OR (t.typcategory = 'U' AND t.typname <> 'uuid')  -- user-defined, but ids are fine
           OR t.typname IN ('json', 'jsonb', 'bytea', 'inet', 'cidr'))
      AND (c.relname || '.' || a.attname) <> ALL (reviewed_columns);

    IF unreviewed IS NOT NULL THEN
        RAISE EXCEPTION 'Unreviewed text columns (may hold personal data): %. Anonymize them or add them to reviewed_columns in anonymize.sql.', unreviewed;
    END IF;
END
$$;
