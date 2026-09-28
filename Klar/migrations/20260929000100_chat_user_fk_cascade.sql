-- conversations.user1_id/user2_id and messages.sender_id referenced users
-- without an ON DELETE action, so DELETE FROM users failed for anyone who
-- had ever chatted -- i.e. account deletion (DELETE /users/me, GDPR Art. 17)
-- returned a 500 for them.
--
-- CASCADE, consistent with every other user-owned table (posts, comments,
-- likes, follows): deleting an account removes its conversations, so the
-- other participant loses that chat history too. The alternative (keeping
-- the partner's copy with a "deleted user" placeholder) would need nullable
-- participant columns and frontend handling; revisit if that's wanted.
ALTER TABLE conversations
    DROP CONSTRAINT conversations_user1_id_fkey,
    ADD CONSTRAINT conversations_user1_id_fkey
        FOREIGN KEY (user1_id) REFERENCES users(id) ON DELETE CASCADE,
    DROP CONSTRAINT conversations_user2_id_fkey,
    ADD CONSTRAINT conversations_user2_id_fkey
        FOREIGN KEY (user2_id) REFERENCES users(id) ON DELETE CASCADE;

ALTER TABLE messages
    DROP CONSTRAINT messages_sender_id_fkey,
    ADD CONSTRAINT messages_sender_id_fkey
        FOREIGN KEY (sender_id) REFERENCES users(id) ON DELETE CASCADE;
