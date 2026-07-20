-- Chat tables. Unchanged structurally from before, just moved to
-- uuid_generate_v7() defaults for consistency with the rest of the schema.
CREATE TABLE IF NOT EXISTS conversations (
    id         UUID PRIMARY KEY DEFAULT uuid_generate_v7(),
    user1_id   UUID NOT NULL REFERENCES users(id),
    user2_id   UUID NOT NULL REFERENCES users(id),
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_unique_conversation
ON conversations (least(user1_id, user2_id), greatest(user1_id, user2_id));

CREATE INDEX IF NOT EXISTS idx_conversations_user1_id ON conversations (user1_id);
CREATE INDEX IF NOT EXISTS idx_conversations_user2_id ON conversations (user2_id);

CREATE TABLE IF NOT EXISTS messages (
    id                   UUID PRIMARY KEY DEFAULT uuid_generate_v7(),
    conversation_id      UUID NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    sender_id            UUID NOT NULL REFERENCES users(id),
    body                 TEXT NOT NULL,
    edited_at            TIMESTAMPTZ,
    reply_to_message_id  UUID REFERENCES messages(id) ON DELETE SET NULL,
    is_read              BOOLEAN DEFAULT false,
    created_at           TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_messages_conversation ON messages (conversation_id, created_at);
CREATE INDEX IF NOT EXISTS idx_messages_sender_id ON messages (sender_id);

CREATE TABLE IF NOT EXISTS message_reactions (
    message_id UUID NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    user_id    UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    emoji      TEXT NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    PRIMARY KEY (message_id, user_id, emoji)
);

CREATE INDEX IF NOT EXISTS idx_message_reactions_lookup ON message_reactions (message_id);
CREATE INDEX IF NOT EXISTS idx_message_reactions_user_id ON message_reactions (user_id);
