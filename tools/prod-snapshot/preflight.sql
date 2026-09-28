-- Checks that only make sense on the *real* data, run by snapshot.sh on
-- the raw import right before anonymize.sql replaces emails, usernames
-- and texts. Aggregates only: every query must return counts, never
-- values, so no personal data is printed.
--
-- Add a check here when a question depends on the original values
-- (e.g. "would this new validation rule reject existing data?").
-- Everything else belongs in checks/, which runs on the anonymized copy.

\pset footer off

-- The backup can be older than the checkout, so a check that needs a
-- column added by a later migration must test for it first (\gset + \if).
SELECT EXISTS (
    SELECT 1 FROM information_schema.columns
    WHERE table_name = 'users' AND column_name = 'terms_accepted_at'
) AS has_terms_accepted_at \gset

\echo '── Accounts'
SELECT
    COUNT(*)                                              AS users,
    COUNT(*) FILTER (WHERE email_verified)                AS verified,
    COUNT(*) FILTER (WHERE is_private)                    AS private
FROM users;
\if :has_terms_accepted_at
SELECT COUNT(*) FILTER (WHERE terms_accepted_at IS NULL) AS without_terms_consent FROM users;
\else
\echo '   (no terms_accepted_at column yet: this backup predates migration 20260825000000)'
\endif

\echo '── Emails (migration 20260929000000 needs duplicate_groups = 0)'
SELECT
    (SELECT COUNT(*) FROM (
        SELECT 1 FROM users GROUP BY LOWER(email) HAVING COUNT(*) > 1
    ) d)                                                  AS duplicate_groups,
    COUNT(*) FILTER (WHERE email <> LOWER(email))         AS mixed_case,
    COUNT(*) FILTER (WHERE email <> btrim(email))         AS surrounding_whitespace
FROM users;

\echo '── Usernames vs. the new rule (^[A-Za-z0-9_]{3,30}$, not reserved)'
SELECT
    COUNT(*) FILTER (WHERE username !~ '^[A-Za-z0-9_]{3,30}$')  AS break_rule,
    COUNT(*) FILTER (WHERE LOWER(username) IN (
        'me', 'search', 'admin', 'administrator', 'mod', 'moderator', 'staff',
        'support', 'help', 'official', 'system', 'root', 'klar', 'klarsocial'
    ))                                                    AS reserved
FROM users;

\echo '── Texts over the new length limits (characters)'
SELECT
    (SELECT COUNT(*) FROM users    WHERE char_length(bio) > 500)       AS bio_over_500,
    (SELECT COUNT(*) FROM posts    WHERE char_length(caption) > 2000)  AS caption_over_2000,
    (SELECT COUNT(*) FROM comments WHERE char_length(body) > 2000)     AS comment_over_2000,
    (SELECT COUNT(*) FROM messages WHERE char_length(body) > 2000)     AS message_over_2000,
    (SELECT COUNT(*) FROM messages WHERE btrim(body) = '')             AS empty_messages;
