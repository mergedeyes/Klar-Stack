-- Content reporting & moderation.
--
-- Design: reports are polymorphic (target_type + target_id, no FK on
-- target_id since it can point at posts, comments, or users) rather
-- than three separate tables, since the review queue needs to list all
-- of them together sorted by severity regardless of target type.
--
-- moderation_status on posts/comments is a *derived, cached* state set
-- automatically by report severity (see handlers/reports.rs) -- 'hidden'
-- for CSAM reports (zero tolerance, no exceptions), 'flagged' for
-- violence/self-harm/sexual-content reports (shown behind an
-- interstitial, not removed -- a single report shouldn't have unilateral
-- takedown power), 'visible' otherwise.

CREATE TYPE report_reason AS ENUM (
    'spam', 'harassment', 'hate_speech', 'violence',
    'self_harm', 'sexual_content', 'csam', 'impersonation', 'other'
);

CREATE TYPE report_target_type AS ENUM ('post', 'comment', 'user');

CREATE TYPE report_status AS ENUM ('pending', 'dismissed', 'actioned');

CREATE TYPE moderation_status AS ENUM ('visible', 'flagged', 'hidden');

CREATE TABLE reports (
    id           UUID NOT NULL DEFAULT uuid_generate_v7() PRIMARY KEY,
    reporter_id  UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    target_type  report_target_type NOT NULL,
    target_id    UUID NOT NULL,
    reason       report_reason NOT NULL,
    details      TEXT,
    status       report_status NOT NULL DEFAULT 'pending',
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    reviewed_at  TIMESTAMPTZ,
    reviewed_by  UUID REFERENCES users(id) ON DELETE SET NULL
);

-- Review queue reads: pending items first, most recent first within
-- that. Severity ordering is computed in the application layer (it
-- depends on `reason`, which isn't naturally sortable), so this index
-- just needs to make the status/created_at filter+sort itself cheap.
CREATE INDEX idx_reports_status_created ON reports (status, created_at DESC);

-- Looking up "does this specific post/comment/user already have pending
-- reports" (e.g. before re-flagging, or to show a count).
CREATE INDEX idx_reports_target ON reports (target_type, target_id);

ALTER TABLE posts ADD COLUMN IF NOT EXISTS moderation_status moderation_status NOT NULL DEFAULT 'visible';
ALTER TABLE comments ADD COLUMN IF NOT EXISTS moderation_status moderation_status NOT NULL DEFAULT 'visible';
