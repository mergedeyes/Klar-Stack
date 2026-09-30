-- Notices about changes to the Terms of Service and the privacy policy
-- (handlers/legal_updates.rs). Each notice is a file in klar-web/legal-updates
-- with a short, plain-language summary, written in the same pull request as
-- the change and published by the frontend deploy once the new page is
-- live (source_key = the file name, so every file is published once).
-- Every account that existed before then sees it
-- in the app on its next visit (Terms changes need an explicit "accept",
-- recorded here as proof), and verified addresses also get an email.
-- Unverified addresses are left out: they may be a typo, i.e. a stranger.

CREATE TABLE legal_updates (
    id                   UUID NOT NULL DEFAULT uuid_generate_v7() PRIMARY KEY,
    published_at         TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- The file in klar-web/legal-updates it came from; NULL when an admin
    -- published it through the API.
    source_key           TEXT UNIQUE,
    -- No foreign key, like the other admin columns; NULL for the deploy.
    published_by         UUID,
    -- 'terms' and/or 'privacy'.
    documents            TEXT[] NOT NULL
                         CHECK (cardinality(documents) > 0 AND documents <@ ARRAY['terms', 'privacy']),
    summary              TEXT NOT NULL,
    -- True for Terms changes: the notice can't be dismissed, only accepted
    -- (or the account deleted). Privacy notices are information only.
    requires_acceptance  BOOLEAN NOT NULL,
    -- Set once every verified address has been emailed.
    emails_finished_at   TIMESTAMPTZ
);

-- Who saw or accepted which notice, and when: the proof that a user agreed
-- to changed Terms. Goes with the account.
CREATE TABLE legal_update_acks (
    update_id        UUID NOT NULL REFERENCES legal_updates(id) ON DELETE CASCADE,
    user_id          UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    acknowledged_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    accepted         BOOLEAN NOT NULL,
    PRIMARY KEY (update_id, user_id)
);

-- One row per email sent (claimed before sending), so a restart resumes
-- where it stopped instead of mailing anyone twice.
CREATE TABLE legal_update_emails (
    update_id  UUID NOT NULL REFERENCES legal_updates(id) ON DELETE CASCADE,
    user_id    UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    sent_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (update_id, user_id)
);
