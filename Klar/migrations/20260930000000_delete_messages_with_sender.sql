-- A deleted account's direct messages are now erased from the partner's
-- chat as well, instead of staying there under "Deleted User" (which
-- 20260929000100 did). The conversation itself survives for the partner,
-- holding only their own messages plus a notice that the account was
-- deleted; if both participants are gone, delete_account removes it.
--
-- Deleting the rows via the FK keeps this atomic with DELETE FROM users.
-- Their reactions go with them (message_reactions ON DELETE CASCADE), and
-- the partner's replies to them keep their text but lose the reference
-- (reply_to_message_id ON DELETE SET NULL).
--
-- sender_id stays nullable only so the compile-checked query! macros in
-- chats.rs keep their Option<Uuid> type; after this migration no row has
-- a NULL sender.

-- Messages left behind by accounts deleted before this migration.
DELETE FROM messages WHERE sender_id IS NULL;

ALTER TABLE messages
    DROP CONSTRAINT messages_sender_id_fkey,
    ADD CONSTRAINT messages_sender_id_fkey
        FOREIGN KEY (sender_id) REFERENCES users(id) ON DELETE CASCADE;
