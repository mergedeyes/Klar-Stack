-- Accounts an admin locked because a takeover is suspected (e.g. a normal
-- account suddenly posting spam), handlers/account_lock.rs.
--
-- Locking ends every session and blocks the account until its owner sets a
-- new password through the link we email them. It is a security measure,
-- not a moderation decision: no statement of reasons, no strike. Each row
-- is also the incident record (Art. 33(5) GDPR: every personal data breach
-- is documented, whether or not it has to be reported), so it outlives the
-- account (user_id is set NULL on deletion) and the admin fills in the
-- assessment: what the intruder could see, the risk, and whether it was
-- reported to the data protection authority.

CREATE TABLE account_locks (
    id                 UUID NOT NULL DEFAULT uuid_generate_v7() PRIMARY KEY,
    user_id            UUID REFERENCES users(id) ON DELETE SET NULL,
    -- No foreign keys on the admins, like moderation_decisions.decided_by.
    locked_by          UUID NOT NULL,
    locked_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Why a takeover is suspected. Internal.
    note               TEXT NOT NULL,
    assessment         TEXT,
    assessed_by        UUID,
    assessed_at        TIMESTAMPTZ,
    -- Reset links sent for this lock (the first with the lock email), for
    -- the resend limit.
    links_sent         INT NOT NULL DEFAULT 1,
    last_link_sent_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    unlocked_at        TIMESTAMPTZ,
    -- 'password_reset' (the owner set a new password) or 'admin'.
    unlocked_via       TEXT CHECK (unlocked_via IN ('password_reset', 'admin')),
    unlocked_by        UUID
);

CREATE UNIQUE INDEX idx_account_locks_active ON account_locks (user_id) WHERE unlocked_at IS NULL;
CREATE INDEX idx_account_locks_locked_at ON account_locks (locked_at DESC);
