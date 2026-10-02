-- Every audit export (handlers/audit_export.rs): who exported which period,
-- whether with identities, and why. An export hands moderation records to
-- someone outside the team, so it leaves the same trail as opening
-- evidence. Kept three years like the decisions it lists (retention.rs).
CREATE TABLE audit_exports (
    id              UUID NOT NULL DEFAULT uuid_generate_v7() PRIMARY KEY,
    -- No foreign key, like rights_claim_events.actor_id: the entry has to
    -- survive the admin's account.
    exported_by     UUID NOT NULL,
    period_from     DATE NOT NULL,
    period_to       DATE NOT NULL,
    with_identities BOOLEAN NOT NULL,
    reason          TEXT NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CHECK (period_from <= period_to)
);

CREATE INDEX idx_audit_exports_created ON audit_exports (created_at DESC);
