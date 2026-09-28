-- Emails are unique case-insensitively: "Foo@x.de" and "foo@x.de" are the
-- same mailbox, so they must be the same account. The original
-- column-level UNIQUE only catches exact-case duplicates.
--
-- New and updated emails are stored lowercased by the application (see
-- validation::normalize_email), and lookups compare LOWER(email) so
-- accounts registered before that with mixed case keep working. This
-- index enforces uniqueness and makes those lookups indexed.
--
-- Migrations run on startup, so a plain CREATE UNIQUE INDEX would crash
-- the backend if case-variant duplicates already exist. In that case the
-- index is skipped with a warning instead; resolve the duplicates
--   SELECT LOWER(email), array_agg(username) FROM users
--   GROUP BY 1 HAVING COUNT(*) > 1;
-- and create it by hand:
--   CREATE UNIQUE INDEX idx_users_email_ci ON users (LOWER(email));
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM users GROUP BY LOWER(email) HAVING COUNT(*) > 1
    ) THEN
        RAISE WARNING 'idx_users_email_ci NOT created: users has emails differing only in case -- see migration 20260929000000';
    ELSE
        CREATE UNIQUE INDEX IF NOT EXISTS idx_users_email_ci ON users (LOWER(email));
    END IF;
END
$$;
