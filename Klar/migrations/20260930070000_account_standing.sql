-- Account standing: strike points for upheld violations, and account-level
-- measures (warning, suspension, permanent suspension).
--
-- When an admin removes reported content, they classify it as one of the
-- violation types in the catalog (standing.rs), each with a written
-- criterion and fixed points, and its author gets a strike worth those
-- points. The strike keeps a snapshot of what was removed and its context,
-- so whoever later decides a measure sees the behaviour, not just a number;
-- the snapshot goes with the strike. Strikes expire after a period that
-- also depends on the severity, and are deleted then. The sum of active
-- strikes, capped at 100, is the account's score; it only *suggests* the
-- next measure -- an admin decides every warning and suspension (DSA
-- Art. 23 asks for a case-by-case assessment, and GDPR Art. 22 restricts
-- fully automated decisions with significant effects).

CREATE TABLE account_strikes (
    id           UUID NOT NULL DEFAULT uuid_generate_v7() PRIMARY KEY,
    user_id      UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    -- The removal the strike is for; an accepted objection to it deletes
    -- the strike.
    decision_id  UUID NOT NULL REFERENCES moderation_decisions(id) ON DELETE CASCADE,
    -- The violation type from the catalog, and its report reason (the
    -- confirmed one, which may differ from what the reporter picked).
    violation    TEXT NOT NULL,
    reason       report_reason NOT NULL,
    severity     TEXT NOT NULL CHECK (severity IN ('minor', 'moderate', 'serious', 'severe')),
    -- Stored rather than derived from the catalog, so changing it later
    -- doesn't rewrite past strikes. points is what counts: base_points,
    -- raised by the repeat factor for a repeated violation.
    base_points  INT NOT NULL CHECK (base_points > 0),
    points       INT NOT NULL CHECK (points >= base_points),
    -- What was removed, with its context (post and parent comment for a
    -- comment) and the reports on it; see standing::snapshot_sql. Images
    -- aren't copied: likely-illegal ones are in the evidence store.
    snapshot     JSONB NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- NULL: doesn't expire (severe violations).
    expires_at   TIMESTAMPTZ
);

CREATE INDEX idx_account_strikes_user ON account_strikes (user_id, created_at DESC);
CREATE INDEX idx_account_strikes_expiry ON account_strikes (expires_at) WHERE expires_at IS NOT NULL;
CREATE UNIQUE INDEX idx_account_strikes_decision ON account_strikes (decision_id);
CREATE INDEX idx_account_strikes_repeat ON account_strikes (user_id, reason, created_at);

-- Every time an admin opens a strike's snapshot. No foreign keys, so the
-- log outlives the strike; deleted after a year by the sweeper.
CREATE TABLE strike_views (
    id         UUID NOT NULL DEFAULT uuid_generate_v7() PRIMARY KEY,
    strike_id  UUID NOT NULL,
    user_id    UUID NOT NULL,
    admin_id   UUID NOT NULL,
    viewed_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX idx_strike_views_strike ON strike_views (strike_id, viewed_at DESC);

-- A suspended account is read-only and its profile and content are hidden
-- from everyone else. 'infinity' is a permanent suspension.
ALTER TABLE users ADD COLUMN IF NOT EXISTS suspended_until TIMESTAMPTZ;
-- The decision that imposed the current suspension, so lifting that
-- decision (objection accepted, lifted early) ends exactly that suspension.
ALTER TABLE users ADD COLUMN IF NOT EXISTS suspension_decision_id UUID
    REFERENCES moderation_decisions(id) ON DELETE SET NULL;

-- Account measures are decisions about target_type 'user'.
ALTER TABLE moderation_decisions DROP CONSTRAINT IF EXISTS moderation_decisions_restriction_check;
ALTER TABLE moderation_decisions ADD CONSTRAINT moderation_decisions_restriction_check
    CHECK (restriction IN ('removed', 'hidden', 'flagged', 'warning', 'suspended', 'banned'));
-- Length of a temporary suspension; NULL for everything else.
ALTER TABLE moderation_decisions ADD COLUMN IF NOT EXISTS suspension_days INT;
-- Who lifted a restriction early (not through an objection). No foreign
-- key, like decided_by, so the record outlives the admin's account.
ALTER TABLE moderation_decisions ADD COLUMN IF NOT EXISTS lifted_by UUID;
-- The score when an account measure was decided, as shown in the statement.
ALTER TABLE moderation_decisions ADD COLUMN IF NOT EXISTS standing_score INT;
-- For a removal: the violation type the admin classified it as ('none':
-- removed without a strike), and their justification when they departed
-- from the reporter's reason or gave no strike. Internal, not shown to the
-- user; the statement shows the type and its criterion.
ALTER TABLE moderation_decisions ADD COLUMN IF NOT EXISTS violation_type TEXT;
ALTER TABLE moderation_decisions ADD COLUMN IF NOT EXISTS violation_note TEXT;
-- A permanently suspended account is deleted once the objection window
-- (six months) has passed without a pending objection, after a reminder
-- two weeks before (standing::sweep_bans). Both steps are recorded on the
-- ban decision, which stays as the audit trail after the account is gone.
ALTER TABLE moderation_decisions ADD COLUMN IF NOT EXISTS deletion_notified_at TIMESTAMPTZ;
ALTER TABLE moderation_decisions ADD COLUMN IF NOT EXISTS account_deleted_at TIMESTAMPTZ;
