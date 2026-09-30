-- Statements of reasons (DSA Art. 17) and notice outcomes (Art. 16).
--
-- Every restriction of someone's content -- removal by an admin, or the
-- automatic hide/warning a report triggers -- gets a decision record. It is
-- the statement shown to the affected user (what was restricted, why, on
-- which ground, whether it was automated, how to object) and at the same
-- time the audit record of the decision. The wording lives in
-- moderation_notice.rs; the record stores the text exactly as it was shown.

CREATE TABLE moderation_decisions (
    id                UUID NOT NULL DEFAULT uuid_generate_v7() PRIMARY KEY,
    target_type       report_target_type NOT NULL,
    target_id         UUID NOT NULL,
    -- The author of the restricted content. NULL once their account is gone.
    affected_user_id  UUID REFERENCES users(id) ON DELETE SET NULL,
    -- 'removed' (deleted by an admin), 'hidden' (automatically hidden after
    -- a CSAM report) or 'flagged' (automatically shown behind a warning).
    restriction       TEXT NOT NULL CHECK (restriction IN ('removed', 'hidden', 'flagged')),
    automated         BOOLEAN NOT NULL,
    reason            report_reason NOT NULL,
    -- 'illegal' (a legal provision) or 'terms' (our Terms of Service), and
    -- the provision or section cited, as shown to the user.
    ground_type       TEXT NOT NULL CHECK (ground_type IN ('illegal', 'terms')),
    ground            TEXT NOT NULL,
    explanation       TEXT NOT NULL,
    -- A short excerpt of what the decision was about, so the statement stays
    -- specific after the content is deleted. Cleared on account deletion.
    content_excerpt   TEXT,
    -- The admin who decided; NULL for automated decisions. No foreign key,
    -- so the record still says who decided after that account is deleted.
    decided_by        UUID,
    report_ids        UUID[] NOT NULL DEFAULT '{}',
    created_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- When the statement was sent to the affected user. NULL while it is
    -- held back: an automatic CSAM hide isn't announced before an admin has
    -- reviewed it, so a suspect isn't alerted before a report to the
    -- authorities (pending legal review).
    delivered_at      TIMESTAMPTZ,
    -- A later decision on the same content (e.g. removal after a warning).
    superseded_by     UUID REFERENCES moderation_decisions(id),
    -- Set when the restriction is lifted: reports dismissed, or an
    -- objection upheld.
    lifted_at         TIMESTAMPTZ,
    -- The affected user's objection and its outcome.
    objection         TEXT,
    objected_at       TIMESTAMPTZ,
    objection_status  TEXT CHECK (objection_status IN ('pending', 'rejected', 'accepted')),
    objection_response TEXT,
    objection_resolved_at TIMESTAMPTZ,
    objection_resolved_by UUID
);

CREATE INDEX idx_moderation_decisions_user ON moderation_decisions (affected_user_id, created_at DESC);
CREATE INDEX idx_moderation_decisions_target ON moderation_decisions (target_type, target_id);
CREATE INDEX idx_moderation_decisions_objections ON moderation_decisions (objected_at)
    WHERE objection_status = 'pending';

-- Moderation notices come from Klar, not from another user, so the actor
-- becomes optional. decision_id links a notice to its statement.
ALTER TABLE notifications ALTER COLUMN actor_id DROP NOT NULL;
ALTER TABLE notifications ADD COLUMN IF NOT EXISTS decision_id UUID
    REFERENCES moderation_decisions(id) ON DELETE CASCADE;

-- 'moderation_decision': a statement of reasons for the affected user.
-- 'report_outcome': a reporter learns what happened to their report.
-- 'objection_resolved': the affected user learns the outcome of their objection.
ALTER TYPE notification_type ADD VALUE IF NOT EXISTS 'moderation_decision';
ALTER TYPE notification_type ADD VALUE IF NOT EXISTS 'report_outcome';
ALTER TYPE notification_type ADD VALUE IF NOT EXISTS 'objection_resolved';
