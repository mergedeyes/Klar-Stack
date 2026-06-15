-- Add email verification status to users
ALTER TABLE users ADD COLUMN email_verified BOOLEAN NOT NULL DEFAULT FALSE;

-- Tokens table for email verification and password reset
-- Used for both flows — the token_type distinguishes them
CREATE TABLE email_tokens (
    id          UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id     UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token       TEXT NOT NULL UNIQUE,
    token_type  VARCHAR(20) NOT NULL, -- 'verification' or 'password_reset'
    expires_at  TIMESTAMPTZ NOT NULL,
    used_at     TIMESTAMPTZ,          -- NULL = unused, timestamp = when it was consumed
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_email_tokens_token ON email_tokens (token);
CREATE INDEX idx_email_tokens_user_id ON email_tokens (user_id);
