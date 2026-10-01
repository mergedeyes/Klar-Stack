-- Recounts the denormalized counters once. Deleting an account removed its
-- likes, comments and follows through ON DELETE CASCADE without lowering the
-- counters on other people's posts, comments and profiles, so they drifted
-- (fixed in users.rs's delete_user). Only rows that are off are touched.
--
-- What counts: comment_count and post_count leave out content removed by
-- moderation (moderation_status 'removed'), which nobody can see any more.

UPDATE posts p SET like_count = x.n
FROM (SELECT p2.id, COUNT(l.user_id) AS n FROM posts p2 LEFT JOIN likes l ON l.post_id = p2.id GROUP BY p2.id) x
WHERE x.id = p.id AND p.like_count != x.n;

UPDATE posts p SET comment_count = x.n
FROM (
    SELECT p2.id, COUNT(c.id) FILTER (WHERE c.moderation_status != 'removed') AS n
    FROM posts p2 LEFT JOIN comments c ON c.post_id = p2.id GROUP BY p2.id
) x
WHERE x.id = p.id AND p.comment_count != x.n;

UPDATE comments c SET like_count = x.n
FROM (SELECT c2.id, COUNT(cl.user_id) AS n FROM comments c2 LEFT JOIN comment_likes cl ON cl.comment_id = c2.id GROUP BY c2.id) x
WHERE x.id = c.id AND c.like_count != x.n;

UPDATE users u SET follower_count = x.followers, following_count = x.following, post_count = x.posts
FROM (
    SELECT u2.id,
        (SELECT COUNT(*) FROM follows f WHERE f.following_id = u2.id) AS followers,
        (SELECT COUNT(*) FROM follows f WHERE f.follower_id = u2.id) AS following,
        (SELECT COUNT(*) FROM posts p WHERE p.user_id = u2.id AND p.moderation_status != 'removed') AS posts
    FROM users u2
) x
WHERE x.id = u.id
  AND (u.follower_count, u.following_count, u.post_count) IS DISTINCT FROM (x.followers, x.following, x.posts);
