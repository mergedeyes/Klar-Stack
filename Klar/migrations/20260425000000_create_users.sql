-- Users table: the foundation of everything
-- Using UUID instead of auto-increment (remember why: no enumeration attacks, works distributed)
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

CREATE TABLE users (
    id          UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    username    VARCHAR(30) UNIQUE NOT NULL,
    email       VARCHAR(255) UNIQUE NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Indexes are already created by UNIQUE constraints above
-- but let's be explicit about what we'll query by
CREATE INDEX idx_users_username ON users (username);