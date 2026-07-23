-- New notification types for the private-account follow-request flow.
-- Written as bare ALTER TYPE ... ADD VALUE IF NOT EXISTS statements (no
-- DO block, nothing else in this file) -- Postgres allows ADD VALUE
-- inside a transaction since PG12, but only as long as the new value
-- isn't *used* in that same transaction. Since nothing else in this
-- migration references these values, that's not a concern here.
ALTER TYPE notification_type ADD VALUE IF NOT EXISTS 'follow_request';
ALTER TYPE notification_type ADD VALUE IF NOT EXISTS 'follow_accepted';
