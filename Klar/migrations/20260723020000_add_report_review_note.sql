-- The review queue lets an admin attach an optional note when dismissing
-- or removing a report (e.g. "false report, content is fine" or "removed
-- per policy X") -- useful context if the same content gets re-reported
-- later, or just for the admin's own record of why a call was made.
ALTER TABLE reports ADD COLUMN IF NOT EXISTS review_note TEXT;
