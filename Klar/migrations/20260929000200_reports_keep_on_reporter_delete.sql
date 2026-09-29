-- Reports survive the reporter's account deletion.
--
-- reporter_id was ON DELETE CASCADE, so deleting an account silently
-- dropped every report that account had filed -- including pending ones
-- still waiting in the review queue, and the history of reports that
-- led to moderation decisions. The report is about the *content*, not
-- the reporter, so it stays; only the link to the deleted account goes
-- (same approach as reviewed_by, and as chats in 20260929000100).
ALTER TABLE reports ALTER COLUMN reporter_id DROP NOT NULL;
ALTER TABLE reports DROP CONSTRAINT reports_reporter_id_fkey;
ALTER TABLE reports
    ADD CONSTRAINT reports_reporter_id_fkey
    FOREIGN KEY (reporter_id) REFERENCES users(id) ON DELETE SET NULL;
