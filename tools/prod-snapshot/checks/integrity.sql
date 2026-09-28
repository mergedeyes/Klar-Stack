-- Consistency checks on the anonymized snapshot. Every row printed is a
-- problem; empty results mean everything is consistent.
--
-- The denormalized counters are maintained by the application alongside
-- the writes that change them (see migrations 20260720000100 users.sql),
-- so drift here points at a code path that forgets to update one.

\pset footer off

\echo '── Posts whose comment_count differs from the actual number of comments'
SELECT p.id, p.comment_count, COUNT(c.id) AS actual
FROM posts p LEFT JOIN comments c ON c.post_id = p.id
GROUP BY p.id HAVING p.comment_count <> COUNT(c.id)
LIMIT 20;

\echo '── Posts whose like_count differs from the actual number of likes'
SELECT p.id, p.like_count, COUNT(l.post_id) AS actual
FROM posts p LEFT JOIN likes l ON l.post_id = p.id
GROUP BY p.id HAVING p.like_count <> COUNT(l.post_id)
LIMIT 20;

\echo '── Comments whose like_count differs from the actual number of likes'
SELECT c.id, c.like_count, COUNT(cl.comment_id) AS actual
FROM comments c LEFT JOIN comment_likes cl ON cl.comment_id = c.id
GROUP BY c.id HAVING c.like_count <> COUNT(cl.comment_id)
LIMIT 20;

\echo '── Users whose post/follower/following counters have drifted'
SELECT u.username,
       u.post_count,      (SELECT COUNT(*) FROM posts   WHERE user_id = u.id)      AS actual_posts,
       u.follower_count,  (SELECT COUNT(*) FROM follows WHERE following_id = u.id) AS actual_followers,
       u.following_count, (SELECT COUNT(*) FROM follows WHERE follower_id = u.id)  AS actual_following
FROM users u
WHERE u.post_count      <> (SELECT COUNT(*) FROM posts   WHERE user_id = u.id)
   OR u.follower_count  <> (SELECT COUNT(*) FROM follows WHERE following_id = u.id)
   OR u.following_count <> (SELECT COUNT(*) FROM follows WHERE follower_id = u.id)
LIMIT 20;

\echo '── Reports pointing at content that no longer exists'
SELECT r.target_type, r.status, COUNT(*) AS reports
FROM reports r
WHERE (r.target_type = 'post'    AND NOT EXISTS (SELECT 1 FROM posts    WHERE id = r.target_id))
   OR (r.target_type = 'comment' AND NOT EXISTS (SELECT 1 FROM comments WHERE id = r.target_id))
   OR (r.target_type = 'user'    AND NOT EXISTS (SELECT 1 FROM users    WHERE id = r.target_id))
GROUP BY 1, 2;

\echo '── Posts by followed users missing from the follower feed (fan-out gaps)'
SELECT COUNT(*) AS missing_feed_items
FROM follows f
JOIN posts p ON p.user_id = f.following_id
WHERE NOT EXISTS (
    SELECT 1 FROM feed_items fi WHERE fi.user_id = f.follower_id AND fi.post_id = p.id
);
