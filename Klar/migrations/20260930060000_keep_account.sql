-- Everything is wiped before launch, except the accounts whose owners
-- asked to keep theirs (Settings -> "Keep my account after the test").
-- The timestamp records when they opted in; opting out clears it.
ALTER TABLE users ADD COLUMN keep_after_test_at TIMESTAMPTZ;
