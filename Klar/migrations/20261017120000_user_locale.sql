-- The language Klar is shown in for an account (and, later, the language
-- of its emails). NULL until the app first sees the account after this
-- change: the frontend then fills it in from the browser's language. New
-- accounts get it at registration; the owner changes it in Settings.
ALTER TABLE users ADD COLUMN locale TEXT CHECK (locale IN ('en', 'de'));
