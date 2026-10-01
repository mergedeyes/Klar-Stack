-- The reporting and moderation workflow, end to end (moderation.rs,
-- handlers/reports.rs, handlers/moderation.rs).

-- 1. Reports
--
-- What the reporter is told about their report (DSA Art. 16(5)). The status
-- alone said "no violation found" for every report that didn't end in a
-- removal, including reports that led to a suspension.
--   removed          the content (or part of the profile) was removed
--   account_measure  the account was warned, suspended or locked
--   no_violation     reviewed, nothing against the rules
--   obsolete         deleted by its author before anyone reviewed it
--   duplicate        the same person had already reported the same thing
ALTER TABLE reports ADD COLUMN IF NOT EXISTS outcome TEXT
    CHECK (outcome IN ('removed', 'account_measure', 'no_violation', 'obsolete', 'duplicate'));
UPDATE reports SET outcome = CASE WHEN status = 'dismissed' THEN 'no_violation' ELSE 'removed' END
WHERE status != 'pending' AND outcome IS NULL;

-- Where a report came from, so a statement of reasons can say whether the
-- decision followed a notice, the team's own initiative or an authority's
-- order (Art. 17(3)(b)). Reports from the team itself have the admin as
-- reporter; a public notice has none (see content_notices).
ALTER TABLE reports ADD COLUMN IF NOT EXISTS source TEXT NOT NULL DEFAULT 'user_report'
    CHECK (source IN ('user_report', 'public_notice', 'own_initiative', 'authority_order'));
-- For an authority's order: which authority, and its reference.
ALTER TABLE reports ADD COLUMN IF NOT EXISTS authority TEXT;
ALTER TABLE reports ADD COLUMN IF NOT EXISTS order_reference TEXT;
-- A reporter can ask once to have a dismissed report checked again.
ALTER TABLE reports ADD COLUMN IF NOT EXISTS recheck_requested_at TIMESTAMPTZ;
ALTER TABLE reports ADD COLUMN IF NOT EXISTS recheck_note TEXT;

-- One pending report per person and item: the same person filing the same
-- report a hundred times added nothing but noise. Duplicates already in the
-- queue are closed, keeping the oldest.
UPDATE reports r
SET status = 'dismissed', outcome = 'duplicate', reviewed_at = NOW(),
    review_note = 'Closed by a migration: the same person had already reported this'
WHERE r.status = 'pending' AND r.reporter_id IS NOT NULL AND EXISTS (
    SELECT 1 FROM reports o
    WHERE o.status = 'pending' AND o.reporter_id = r.reporter_id
      AND o.target_type = r.target_type AND o.target_id = r.target_id
      AND (o.created_at, o.id) < (r.created_at, r.id)
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_reports_pending_once ON reports (reporter_id, target_type, target_id)
    WHERE status = 'pending' AND reporter_id IS NOT NULL;
-- The per-reporter daily limits and the count of dismissed critical reports.
CREATE INDEX IF NOT EXISTS idx_reports_reporter ON reports (reporter_id, created_at DESC);

-- 2. Notices from the public form (handlers/notices.rs): anyone, with or
-- without an account, can report illegal content (DSA Art. 16(1)). The
-- notice holds what the notifier gave us; the report it creates goes through
-- the normal queue, and its outcome is copied back here for the notifier's
-- status page and email. Name and email are optional for child sexual abuse
-- material (Art. 16(2)(c)). Deleted with the retention of reports.
CREATE TABLE IF NOT EXISTS content_notices (
    id                 UUID NOT NULL DEFAULT uuid_generate_v7() PRIMARY KEY,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    reason             report_reason NOT NULL,
    explanation        TEXT NOT NULL,
    content_url        TEXT NOT NULL,
    target_type        report_target_type NOT NULL,
    target_id          UUID NOT NULL,
    notifier_name      TEXT,
    notifier_email     TEXT,
    -- The notifier's account, if they were signed in.
    notifier_user_id   UUID REFERENCES users(id) ON DELETE SET NULL,
    good_faith         BOOLEAN NOT NULL CHECK (good_faith),
    -- SHA-256 of the notifier's status token, like rights claims.
    status_token_hash  TEXT NOT NULL,
    decided_at         TIMESTAMPTZ,
    outcome            TEXT
);
ALTER TABLE reports ADD COLUMN IF NOT EXISTS notice_id UUID REFERENCES content_notices(id) ON DELETE SET NULL;
CREATE INDEX IF NOT EXISTS idx_reports_notice ON reports (notice_id) WHERE notice_id IS NOT NULL;

-- 3. Decisions
--
-- An objection can also end without an answer: 'superseded' when a removal
-- replaced the automatic restriction it was about (the user can object to
-- the removal itself), 'withdrawn' when its author deleted the account.
ALTER TABLE moderation_decisions DROP CONSTRAINT IF EXISTS moderation_decisions_objection_status_check;
ALTER TABLE moderation_decisions ADD CONSTRAINT moderation_decisions_objection_status_check
    CHECK (objection_status IN ('pending', 'rejected', 'accepted', 'superseded', 'withdrawn'));

-- What the decision followed, as named in the statement (Art. 17(3)(b)):
-- a notice (a report or a public notice), the team's own initiative, an
-- authority's order, or a rights claim. NULL for account measures from the
-- standing page or a review, which rest on their own explanation.
ALTER TABLE moderation_decisions ADD COLUMN IF NOT EXISTS source TEXT
    CHECK (source IN ('notice', 'own_initiative', 'authority_order', 'rights_claim'));
UPDATE moderation_decisions
SET source = CASE WHEN rights_claim_id IS NOT NULL THEN 'rights_claim' ELSE 'notice' END
WHERE source IS NULL AND (rights_claim_id IS NOT NULL OR cardinality(report_ids) > 0);

-- For a removal from a profile (handlers/profile_moderation.rs): which parts
-- were removed, with the removed texts and the old username, so an accepted
-- objection can put them back.
ALTER TABLE moderation_decisions ADD COLUMN IF NOT EXISTS removed_fields JSONB;

-- A removed post or comment stays, invisible, until the objection window has
-- passed, so an accepted objection can restore it; then the sweeper deletes
-- it for good and records when. Removals before this migration deleted at
-- once.
ALTER TABLE moderation_decisions ADD COLUMN IF NOT EXISTS content_purged_at TIMESTAMPTZ;
UPDATE moderation_decisions SET content_purged_at = created_at
WHERE restriction = 'removed' AND content_purged_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_moderation_decisions_purge ON moderation_decisions (created_at)
    WHERE restriction = 'removed' AND content_purged_at IS NULL;

-- Lifting a decision when its reports are dismissed looks decisions up by
-- report id.
CREATE INDEX IF NOT EXISTS idx_moderation_decisions_report_ids ON moderation_decisions USING GIN (report_ids);

-- Decisions are deleted after their retention period (retention.rs); a
-- later decision or a rights claim pointing at one mustn't block that.
ALTER TABLE moderation_decisions DROP CONSTRAINT IF EXISTS moderation_decisions_superseded_by_fkey;
ALTER TABLE moderation_decisions ADD CONSTRAINT moderation_decisions_superseded_by_fkey
    FOREIGN KEY (superseded_by) REFERENCES moderation_decisions(id) ON DELETE SET NULL;
ALTER TABLE rights_claims DROP CONSTRAINT IF EXISTS rights_claims_decision_id_fkey;
ALTER TABLE rights_claims ADD CONSTRAINT rights_claims_decision_id_fkey
    FOREIGN KEY (decision_id) REFERENCES moderation_decisions(id) ON DELETE SET NULL;

-- 4. Accounts and sessions
--
-- Set when every session of the account must end at once: a lock, a new
-- password. Access tokens issued before it stop working (auth middleware),
-- and open notification streams close.
ALTER TABLE users ADD COLUMN IF NOT EXISTS sessions_revoked_at TIMESTAMPTZ;
-- Accounts that never verified their email address are deleted after 30
-- days (retention.rs); this is when the reminder went out.
ALTER TABLE users ADD COLUMN IF NOT EXISTS verification_reminder_at TIMESTAMPTZ;
CREATE INDEX IF NOT EXISTS idx_users_unverified ON users (created_at) WHERE NOT email_verified;

-- Refresh tokens come in families: every rotation stays in the family of the
-- login it started from, and a rotated token is kept for a while instead of
-- deleted. If one turns up again after it was rotated, someone else has a
-- copy, so the whole family is revoked (handlers/auth.rs).
ALTER TABLE refresh_tokens ADD COLUMN IF NOT EXISTS family_id UUID;
UPDATE refresh_tokens SET family_id = id WHERE family_id IS NULL;
ALTER TABLE refresh_tokens ALTER COLUMN family_id SET NOT NULL;
ALTER TABLE refresh_tokens ALTER COLUMN family_id SET DEFAULT uuid_generate_v7();
ALTER TABLE refresh_tokens ADD COLUMN IF NOT EXISTS rotated_at TIMESTAMPTZ;
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_family ON refresh_tokens (family_id);

-- 5. Alerts for admins (alerts.rs): one row per alert email sent, claimed
-- before sending, so replicas don't send the same alert twice. Deleted after
-- 30 days.
CREATE TABLE IF NOT EXISTS admin_alerts (
    kind     TEXT NOT NULL,
    key      TEXT NOT NULL,
    sent_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (kind, key)
);
