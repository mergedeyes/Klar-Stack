-- The newest "What's new" release (klar-web/src/whats-new/releases.ts) the
-- account has seen, by its id: the release date, "YYYY-MM-DD", with a
-- suffix for a second one on the same day. NULL until the first one is
-- closed; GET /users/me then reports the account's sign-up date instead,
-- so nobody is shown news from before they joined.
ALTER TABLE users ADD COLUMN whats_new_seen TEXT;
