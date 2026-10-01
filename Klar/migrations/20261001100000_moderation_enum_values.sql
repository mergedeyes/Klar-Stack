-- New values for the moderation workflow (see the next migration). They get a
-- migration of their own because Postgres can't use an enum value in the
-- transaction that adds it.
--
-- report_status 'obsolete': a report closed without a decision because its
-- content was deleted by its author before anyone reviewed it.
-- report_target_type 'message': direct messages can be reported too.
-- moderation_status 'removed': content removed by the moderation team. It
-- stays in the database, invisible to everyone, until the objection window
-- has passed, so an accepted objection can restore it.
ALTER TYPE report_status ADD VALUE IF NOT EXISTS 'obsolete';
ALTER TYPE report_target_type ADD VALUE IF NOT EXISTS 'message';
ALTER TYPE moderation_status ADD VALUE IF NOT EXISTS 'removed';
