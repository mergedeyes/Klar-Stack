-- Rights claims (copyright and similar): formal notices from rightsholders,
-- separate from ordinary user reports.
--
-- Anyone can file one through the public form at /rights, account or not
-- (DSA Art. 16(1)), with their name, email, the post, the work and why they
-- hold the rights, and a good-faith statement (Art. 16(2)). The claimant
-- follows the claim through a private status link; only a hash of its
-- token is stored. An accepted claim hides the post rather than deleting
-- it, so it can be restored if the uploader's objection succeeds.

ALTER TYPE report_reason ADD VALUE IF NOT EXISTS 'copyright';

CREATE TABLE rights_claims (
    id                    UUID NOT NULL DEFAULT uuid_generate_v7() PRIMARY KEY,
    created_at            TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- 'copyright', 'trademark' or 'other' (another right, e.g. to one's own image).
    claim_type            TEXT NOT NULL CHECK (claim_type IN ('copyright', 'trademark', 'other')),

    -- Who claims, and for whom.
    claimant_name         TEXT NOT NULL,
    claimant_email        TEXT NOT NULL,
    claimant_organization TEXT,
    -- Set when the rightsholder is someone else (an agency, a label, ...).
    represented_party     TEXT,
    -- The claimant's Klar account, if they were signed in.
    claimant_user_id      UUID REFERENCES users(id) ON DELETE SET NULL,

    -- What is claimed.
    content_url           TEXT NOT NULL,
    target_type           report_target_type NOT NULL,
    target_id             UUID NOT NULL,
    work_description      TEXT NOT NULL,
    ownership_basis       TEXT NOT NULL,
    original_url          TEXT,
    good_faith            BOOLEAN NOT NULL CHECK (good_faith),

    -- 'submitted' -> 'triaged' -> ('evidence_requested' ->) 'accepted' |
    -- 'declined'; an accepted claim becomes 'restored' when the uploader's
    -- objection succeeds.
    status                TEXT NOT NULL DEFAULT 'submitted'
                          CHECK (status IN ('submitted', 'triaged', 'evidence_requested', 'accepted', 'declined', 'restored')),
    -- Shown to the claimant: what evidence we asked for, their answer, and
    -- the reason for a decline.
    evidence_request      TEXT,
    claimant_response     TEXT,
    decision_reason       TEXT,
    decided_at            TIMESTAMPTZ,
    decided_by            UUID,
    -- The statement of reasons sent to the uploader when accepted.
    decision_id           UUID REFERENCES moderation_decisions(id),
    -- SHA-256 of the claimant's status token.
    status_token_hash     TEXT NOT NULL
);

CREATE INDEX idx_rights_claims_status ON rights_claims (status, created_at);
CREATE INDEX idx_rights_claims_target ON rights_claims (target_type, target_id);

-- Every step, append-only, as the audit trail of the claim. actor_id has no
-- foreign key so it survives account deletion; NULL is the claimant or the
-- system.
CREATE TABLE rights_claim_events (
    id          UUID NOT NULL DEFAULT uuid_generate_v7() PRIMARY KEY,
    claim_id    UUID NOT NULL REFERENCES rights_claims(id) ON DELETE CASCADE,
    actor_id    UUID,
    -- 'submitted', 'triaged', 'evidence_requested', 'claimant_responded',
    -- 'accepted', 'declined', 'restored'
    action      TEXT NOT NULL,
    note        TEXT,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_rights_claim_events_claim ON rights_claim_events (claim_id, created_at);

-- A statement of reasons can stem from a rights claim instead of a report.
ALTER TABLE moderation_decisions ADD COLUMN IF NOT EXISTS rights_claim_id UUID REFERENCES rights_claims(id) ON DELETE SET NULL;
