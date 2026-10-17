-- New accounts get a short guide to Klar after their first sign-in
-- (klar-web WelcomeGuide). The timestamp records when it was finished or
-- closed, so it shows once per account rather than once per device.
-- Accounts that already exist know their way around, so they count as
-- having seen it; the footer link still opens it for anyone.
ALTER TABLE users ADD COLUMN guide_seen_at TIMESTAMPTZ;
UPDATE users SET guide_seen_at = NOW();
