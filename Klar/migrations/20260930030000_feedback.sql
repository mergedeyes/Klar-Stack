-- In-app feedback for the friend-and-family test: bug reports and ideas
-- from signed-in users, read by admins at /admin/feedback.
--
-- Technical context (page, browser, screen size) is only stored when the
-- sender leaves "include technical details" on. Feedback is deleted a year
-- after it was sent (see handlers/feedback.rs); on account deletion only
-- the link to the account goes, since the text is about the app, not them.

CREATE TABLE feedback (
    id          UUID NOT NULL DEFAULT uuid_generate_v7() PRIMARY KEY,
    user_id     UUID REFERENCES users(id) ON DELETE SET NULL,
    category    TEXT NOT NULL CHECK (category IN ('bug', 'idea', 'other')),
    message     TEXT NOT NULL,
    page_path   TEXT,
    user_agent  TEXT,
    viewport    TEXT,
    status      TEXT NOT NULL DEFAULT 'new' CHECK (status IN ('new', 'seen', 'done')),
    admin_note  TEXT,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_feedback_status_created ON feedback (status, created_at DESC);
CREATE INDEX idx_feedback_user ON feedback (user_id, created_at DESC);
