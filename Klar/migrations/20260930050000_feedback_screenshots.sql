-- Screenshots attached to feedback (up to 3 per entry). Screenshots often
-- show other people's posts or messages, so they're kept much shorter than
-- the feedback text: deleted 30 days after the feedback is marked done,
-- 90 days after sending at the latest, and at once when the sender deletes
-- their account (see handlers/feedback.rs). The files live in the media
-- storage under random keys and are only served through the admin API.

CREATE TABLE feedback_screenshots (
    id           UUID NOT NULL DEFAULT uuid_generate_v7() PRIMARY KEY,
    feedback_id  UUID NOT NULL REFERENCES feedback(id) ON DELETE CASCADE,
    storage_key  TEXT NOT NULL,
    width        INT NOT NULL,
    height       INT NOT NULL,
    sort_order   INT NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_feedback_screenshots_feedback ON feedback_screenshots (feedback_id, sort_order);

-- When the feedback was last marked done; the screenshots' 30 days count
-- from here. Cleared again if it's reopened.
ALTER TABLE feedback ADD COLUMN done_at TIMESTAMPTZ;
UPDATE feedback SET done_at = NOW() WHERE status = 'done';
