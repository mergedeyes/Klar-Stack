-- Add edit tracking to posts and comments
-- NULL = never edited, timestamp = when it was last edited
ALTER TABLE posts ADD COLUMN edited_at TIMESTAMPTZ;
ALTER TABLE comments ADD COLUMN edited_at TIMESTAMPTZ;
