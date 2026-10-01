-- The behaviour log goes. post_events recorded per account which posts were
-- liked, unliked and commented on (and offered POST /events for views) "for
-- future ranking", while Klar promises a chronological feed without ranking.
-- Nothing ever read it, so keeping it would only be collecting data without
-- a purpose (Art. 5(1)(b) and (c) GDPR). Dropping the table drops its
-- monthly partitions with it.
DROP TABLE IF EXISTS post_events CASCADE;
DROP FUNCTION IF EXISTS create_post_events_partition(INT, INT);
