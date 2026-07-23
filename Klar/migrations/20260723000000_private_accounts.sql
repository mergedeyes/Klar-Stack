-- Private accounts: adds the flag itself, plus a separate table for
-- *pending* follow requests. Kept separate from `follows` (rather than
-- adding a status column there) so every existing query against `follows`
-- keeps meaning exactly what it already means -- an active, accepted
-- follow relationship -- without needing to touch every one of those
-- queries to add a status filter.
ALTER TABLE users ADD COLUMN IF NOT EXISTS is_private BOOLEAN NOT NULL DEFAULT FALSE;

CREATE TABLE IF NOT EXISTS follow_requests (
    id           UUID NOT NULL DEFAULT uuid_generate_v7(),
    requester_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    target_id    UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (requester_id, target_id)
);

-- Listing "who wants to follow me" (target account checking their own
-- pending requests) is the only real access pattern here.
CREATE INDEX IF NOT EXISTS idx_follow_requests_target ON follow_requests (target_id, created_at DESC);
