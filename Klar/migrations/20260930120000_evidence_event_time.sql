-- Evidence events written in the same transaction (e.g. "content deleted"
-- and "authority report advised" during one removal) all got the
-- transaction's NOW(), and ids from uuid_generate_v7 aren't ordered within
-- a millisecond, so the audit trail could list them in the wrong order.
-- clock_timestamp() records when each event was actually written.
ALTER TABLE evidence_events ALTER COLUMN created_at SET DEFAULT clock_timestamp();
