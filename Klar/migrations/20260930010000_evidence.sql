-- Evidence preservation for likely-illegal content.
--
-- When content gets a report for a likely-illegal reason (CSAM, violence,
-- hate speech, ...), evidence.rs opens an evidence record for it and copies
-- the reported state right away: the item itself (with its images), its
-- author and, for comments, what it replied to -- nothing else. Every edit
-- of the item while the record is open adds a version, and deleting the
-- item no longer destroys the evidence (GDPR Art. 17(3)(e): establishment,
-- exercise or defence of legal claims). Other report reasons (spam, ...)
-- copy nothing.
--
-- A record is open until its last likely-illegal report is resolved. It is
-- then purged straight away if all reports were dismissed, or kept for the
-- retention period if any was actioned, unless a legal hold is set.
--
-- evidence_records is never deleted from: purging deletes the versions and
-- files, while the row and its events stay as the audit trail of what was
-- kept, who looked at it and when it was erased.

CREATE TABLE evidence_records (
    id                UUID NOT NULL DEFAULT uuid_generate_v7() PRIMARY KEY,
    target_type       report_target_type NOT NULL,
    target_id         UUID NOT NULL,
    -- Likely-illegal reasons reported for the target. Kept after the purge:
    -- they say what kind of case it was without holding personal data.
    reasons           TEXT[] NOT NULL DEFAULT '{}',
    created_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Set when the original is deleted: by moderation ('moderation_removal'),
    -- by its author or a post owner ('user_deletion'), or with the account
    -- ('account_deletion').
    content_deleted_at TIMESTAMPTZ,
    deletion_trigger  TEXT CHECK (deletion_trigger IN ('moderation_removal', 'user_deletion', 'account_deletion')),
    -- Set when the last pending likely-illegal report on the target is
    -- resolved: 'removed' if any report on it was actioned, else 'dismissed'.
    decision          TEXT CHECK (decision IN ('removed', 'dismissed')),
    decided_at        TIMESTAMPTZ,
    decided_by        UUID,
    decision_note     TEXT,
    -- NULL while undecided. The sweeper purges once this has passed,
    -- unless legal_hold is set.
    retain_until      TIMESTAMPTZ,
    legal_hold        BOOLEAN NOT NULL DEFAULT FALSE,
    purged_at         TIMESTAMPTZ
);

-- At most one open record per target, so concurrent reports on the same
-- item share one timeline instead of racing to create two.
CREATE UNIQUE INDEX idx_evidence_records_open ON evidence_records (target_type, target_id)
    WHERE decided_at IS NULL AND purged_at IS NULL;
-- The admin queue and deciding look records up by target.
CREATE INDEX idx_evidence_records_target ON evidence_records (target_type, target_id);
-- The sweeper's scan: live records whose retention has passed.
CREATE INDEX idx_evidence_records_due ON evidence_records (retain_until) WHERE purged_at IS NULL;

-- The timeline: one row per captured state of the item.
CREATE TABLE evidence_versions (
    id           UUID NOT NULL DEFAULT uuid_generate_v7() PRIMARY KEY,
    evidence_id  UUID NOT NULL REFERENCES evidence_records(id) ON DELETE CASCADE,
    captured_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- 'reported' (state when the first likely-illegal report came in),
    -- 'edited' (state after an edit), or 'deleted' (state at deletion, only
    -- for reports filed before capture-on-report existed).
    cause        TEXT NOT NULL CHECK (cause IN ('reported', 'edited', 'deleted')),
    -- The item, its author and (for comments) what it replied to.
    content      JSONB NOT NULL
);

CREATE INDEX idx_evidence_versions_evidence ON evidence_versions (evidence_id, captured_at);

CREATE TABLE evidence_files (
    id           UUID NOT NULL DEFAULT uuid_generate_v7() PRIMARY KEY,
    evidence_id  UUID NOT NULL REFERENCES evidence_records(id) ON DELETE CASCADE,
    version_id   UUID NOT NULL REFERENCES evidence_versions(id) ON DELETE CASCADE,
    -- 'post_media' or 'avatar'.
    kind         TEXT NOT NULL,
    sort_order   INTEGER NOT NULL DEFAULT 0,
    -- The file's key in the media storage it came from. Until the copy
    -- succeeds (copied_at set) that original is never deleted.
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

CREATE INDEX idx_evidence_files_version ON evidence_files (version_id, sort_order);
CREATE INDEX idx_evidence_files_evidence ON evidence_files (evidence_id);
-- Deleting media checks whether a pending copy still needs the original.
CREATE INDEX idx_evidence_files_pending_source ON evidence_files (source_key) WHERE copied_at IS NULL;

-- Every access and change, append-only. actor_id deliberately has no
-- foreign key: the trail must still say who acted after that account is
-- deleted. NULL means the system (sweeper).
CREATE TABLE evidence_events (
    id           UUID NOT NULL DEFAULT uuid_generate_v7() PRIMARY KEY,
    evidence_id  UUID NOT NULL REFERENCES evidence_records(id),
    actor_id     UUID,
    -- 'created', 'version_added', 'content_deleted', 'decided', 'viewed',
    -- 'file_viewed', 'hold_set', 'hold_lifted', 'authority_report', 'purged'
    action       TEXT NOT NULL,
    reason       TEXT,
    details      JSONB,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_evidence_events_evidence ON evidence_events (evidence_id, created_at);
