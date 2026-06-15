-- 1. Messages-Tabelle erweitern für Edits und Antworten
ALTER TABLE messages 
ADD COLUMN edited_at TIMESTAMPTZ DEFAULT NULL,
ADD COLUMN reply_to_message_id UUID REFERENCES messages(id) ON DELETE SET NULL;

-- 2. Tabelle für Emoji-Reaktionen anlegen
CREATE TABLE message_reactions (
    message_id UUID NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    emoji TEXT NOT NULL, -- Speichert das Emoji direkt als UTF-8 String
    created_at TIMESTAMPTZ DEFAULT NOW(),
    PRIMARY KEY (message_id, user_id, emoji)
);

-- Index für schnelles Laden der Reaktionen zu einer Nachricht
CREATE INDEX idx_message_reactions_lookup ON message_reactions(message_id);