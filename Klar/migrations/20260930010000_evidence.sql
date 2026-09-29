-- Evidence preservation for likely-illegal content.
--
-- Deleting reported content (admin removal, the author deleting it, or
-- account deletion) used to destroy it together with any evidence of a
-- crime. When such content has a pending report for a likely-illegal
-- reason, evidence.rs now copies it here first (GDPR Art. 17(3)(e):
-- establishment, exercise or defence of legal claims).
--
-- evidence_records is never deleted from. When the retention period ends
-- (or the reports are dismissed) the sweeper purges it: content is set to
-- NULL and the files are deleted, while the row and its events stay as the
-- audit trail of what was kept, who looked at it and when it was erased.

CREATE TABLE evidence_records (
    id            UUID NOT NULL DEFAULT uuid_generate_v7() PRIMARY KEY,
    target_type   report_target_type NOT NULL,
    target_id     UUID NOT NULL,
    -- What deleted the original: 'moderation_removal', 'user_deletion'
    -- (the author, or a post owner deleting a comment on their post) or
    -- 'account_deletion'.
    trigger       TEXT NOT NULL CHECK (trigger IN ('moderation_removal', 'user_deletion', 'account_deletion')),
    -- Report reasons at preservation time. Kept after the purge: they say
    -- what kind of case it was without holding any personal data.
    reasons       TEXT[] NOT NULL,
    -- The preserved snapshot: the content itself, its author, the context
    -- (post / parent comment for comments) and every report on it. NULL
    -- once purged.
    content       JSONB,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Set when the last pending report on the target is resolved: 'removed'
    -- if any report on it was actioned, 'dismissed' otherwise.
    decision      TEXT CHECK (decision IN ('removed', 'dismissed')),
    decided_at    TIMESTAMPTZ,
    decided_by    UUID,
    decision_note TEXT,
    -- NULL while undecided. The sweeper purges once this has passed,
    -- unless legal_hold is set.
    retain_until  TIMESTAMPTZ,
    legal_hold    BOOLEAN NOT NULL DEFAULT FALSE,
    purged_at     TIMESTAMPTZ
);

-- Deciding and the admin queue look records up by target.
CREATE INDEX idx_evidence_records_target ON evidence_records (target_type, target_id);
-- The sweeper's scan: live records whose retention has passed.
CREATE INDEX idx_evidence_records_due ON evidence_records (retain_until) WHERE purged_at IS NULL;

CREATE TABLE evidence_files (
    id           UUID NOT NULL DEFAULT uuid_generate_v7() PRIMARY KEY,
    evidence_id  UUID NOT NULL REFERENCES evidence_records(id) ON DELETE CASCADE,
    -- 'post_media' or 'avatar'.
    kind         TEXT NOT NULL,
    sort_order   INTEGER NOT NULL DEFAULT 0,
    -- The file's key in the media storage it came from. Until the copy
    -- succeeds (copied_at set) that original is kept and not deleted.
    source_key   TEXT NOT NULL,
    -- Key in the evidence storage zone.
    storage_key  TEXT NOT NULL,
    content_type TEXT NOT NULL,
    -- Size and SHA-256 of the copied bytes, so it can later be shown that
    -- the file handed to an authority is the one that was preserved.
    size_bytes   BIGINT,
    sha256       TEXT,
    copied_at    TIMESTAMPTZ
);

CREATE INDEX idx_evidence_files_evidence ON evidence_files (evidence_id, sort_order);
CREATE INDEX idx_evidence_files_pending ON evidence_files (id) WHERE copied_at IS NULL;

-- Every access and change, append-only. actor_id deliberately has no
-- foreign key: the trail must still say who acted after that admin's
-- account is deleted. NULL means the system (sweeper, deletion paths).
CREATE TABLE evidence_events (
    id           UUID NOT NULL DEFAULT uuid_generate_v7() PRIMARY KEY,
    evidence_id  UUID NOT NULL REFERENCES evidence_records(id),
    actor_id     UUID,
    -- 'created', 'decided', 'viewed', 'file_viewed', 'hold_set',
    -- 'hold_lifted', 'authority_report', 'purged'
    action       TEXT NOT NULL,
    reason       TEXT,
    details      JSONB,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_evidence_events_evidence ON evidence_events (evidence_id, created_at);
