-- 1. A "grave" strike severity (60 points, one year) between serious (40)
--    and severe (100): hard, but not an instant ban (standing.rs).
ALTER TABLE account_strikes DROP CONSTRAINT IF EXISTS account_strikes_severity_check;
ALTER TABLE account_strikes ADD CONSTRAINT account_strikes_severity_check
    CHECK (severity IN ('minor', 'moderate', 'serious', 'grave', 'severe'));

-- 2. Account reviews (handlers/account_review.rs): an admin looks at one
--    account's recent activity as a whole to decide whether it was taken
--    over, is a bot, or is fine. Seeing everything someone did on one page
--    is more intrusive than any single item, so opening a review needs a
--    reason and is recorded here (who, when, why); the decision closes it.
--    The row is the audit trail of the review; the lock or ban it leads to
--    has its own record. Deleted a year after it was opened.
CREATE TABLE account_reviews (
    id            UUID NOT NULL DEFAULT uuid_generate_v7() PRIMARY KEY,
    user_id       UUID REFERENCES users(id) ON DELETE SET NULL,
    -- No foreign key, like the other admin columns.
    reviewer_id   UUID NOT NULL,
    reason        TEXT NOT NULL,
    -- The report the review started from, if any.
    report_id     UUID REFERENCES reports(id) ON DELETE SET NULL,
    opened_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- 'no_action', 'locked' (suspected takeover) or 'bot' (bot or
    -- spam-only account, permanently suspended).
    outcome       TEXT CHECK (outcome IN ('no_action', 'locked', 'bot')),
    outcome_note  TEXT,
    decided_at    TIMESTAMPTZ
);

CREATE INDEX idx_account_reviews_user ON account_reviews (user_id, opened_at DESC);
CREATE INDEX idx_account_reviews_opened ON account_reviews (opened_at);
