-- Chats survive account deletion: when one participant deletes their
-- account, the other keeps the conversation and all its messages, shown
-- with a "Deleted user" placeholder.
--
-- Before this, conversations.user1_id/user2_id and messages.sender_id
-- referenced users with no ON DELETE action, so DELETE FROM users failed
-- outright for anyone who had ever chatted (account deletion, GDPR
-- Art. 17, returned a 500).
--
-- Now the deleted side becomes NULL. The deleted user's message texts stay
-- in the partner's copy of the chat (they were sent to that person); if
-- legal review decides they must be erased instead, blank messages.body
-- WHERE sender_id IS NULL in delete_account.
--
-- A conversation whose *both* participants are gone is deleted by the
-- application (users.rs's delete_account), not left orphaned here.

ALTER TABLE conversations
    ALTER COLUMN user1_id DROP NOT NULL,
    ALTER COLUMN user2_id DROP NOT NULL,
    DROP CONSTRAINT conversations_user1_id_fkey,
    ADD CONSTRAINT conversations_user1_id_fkey
        FOREIGN KEY (user1_id) REFERENCES users(id) ON DELETE SET NULL,
    DROP CONSTRAINT conversations_user2_id_fkey,
    ADD CONSTRAINT conversations_user2_id_fkey
        FOREIGN KEY (user2_id) REFERENCES users(id) ON DELETE SET NULL;

ALTER TABLE messages
    ALTER COLUMN sender_id DROP NOT NULL,
    DROP CONSTRAINT messages_sender_id_fkey,
    ADD CONSTRAINT messages_sender_id_fkey
        FOREIGN KEY (sender_id) REFERENCES users(id) ON DELETE SET NULL;

-- LEAST/GREATEST skip NULLs, so after deletions conversations (NULL, P)
-- and (P, NULL) would both index as (P, P) and collide -- the second
-- partner's account deletion would fail on this index. Uniqueness only
-- matters between two existing users anyway (it's what send_message's
-- upsert relies on), so the index only covers those rows.
DROP INDEX IF EXISTS idx_unique_conversation;
CREATE UNIQUE INDEX idx_unique_conversation
    ON conversations (least(user1_id, user2_id), greatest(user1_id, user2_id))
    WHERE user1_id IS NOT NULL AND user2_id IS NOT NULL;
