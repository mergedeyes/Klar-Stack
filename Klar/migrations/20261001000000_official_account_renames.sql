-- Renames of official accounts by an admin, handlers/official_accounts.rs.
--
-- An official account is one with a verified address at klarsocial.eu, e.g.
-- kontakt@ for the "Klar" profile. Only these may have the staff names the
-- sign-up form refuses ("klar", "support", ...), and only an admin can give
-- them one. Each row records who renamed which account, from what to what,
-- when and why. It outlives the account (user_id is set NULL on deletion),
-- so the history of who could appear as staff stays complete.

CREATE TABLE official_account_renames (
    id            UUID NOT NULL DEFAULT uuid_generate_v7() PRIMARY KEY,
    user_id       UUID REFERENCES users(id) ON DELETE SET NULL,
    -- No foreign key on the admin, like account_locks.locked_by.
    renamed_by    UUID NOT NULL,
    old_username  VARCHAR(30) NOT NULL,
    new_username  VARCHAR(30) NOT NULL,
    reason        TEXT NOT NULL,
    renamed_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_official_account_renames_renamed_at ON official_account_renames (renamed_at DESC);
