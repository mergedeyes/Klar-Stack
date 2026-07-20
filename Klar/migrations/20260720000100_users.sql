-- Users table.
--
-- username is stored exactly as the person typed it (case preserved) --
-- but uniqueness and every lookup are case-insensitive, so "JohnDoe" and
-- "johndoe" are the same account and the same URL. That's what the
-- unique index on LOWER(username) below enforces (replacing a plain
-- column-level UNIQUE, which would only catch exact-case duplicates);
-- application queries that look a user up by username must match this
-- by comparing LOWER(username) = LOWER($1) rather than a plain equals.
--
-- follower_count / following_count / post_count are denormalized counters,
-- maintained by the application (in the same transaction as the follow/
-- post insert or delete) rather than recomputed with COUNT(*) on every
-- profile view. At small scale COUNT(*) is fine; at millions of rows,
-- doing it on every profile render is real, avoidable I/O.
CREATE TABLE IF NOT EXISTS users (
    id                   UUID PRIMARY KEY DEFAULT uuid_generate_v7(),
    username             VARCHAR(30) NOT NULL,
    email                VARCHAR(255) UNIQUE NOT NULL,
    password_hash        TEXT,
    display_name         VARCHAR(50),
    bio                  TEXT,
    avatar_url           TEXT,
    email_verified       BOOLEAN NOT NULL DEFAULT FALSE,
    username_changed_at  TIMESTAMPTZ,
    follower_count       BIGINT NOT NULL DEFAULT 0,
    following_count      BIGINT NOT NULL DEFAULT 0,
    post_count           BIGINT NOT NULL DEFAULT 0,
    created_at           TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_users_username_ci ON users (LOWER(username));

