DO $$
BEGIN
    CREATE TYPE notification_type AS ENUM (
        'follow',
        'post_like',
        'comment',
        'comment_like'
    );
EXCEPTION
    WHEN duplicate_object THEN NULL;
END $$;

CREATE TABLE IF NOT EXISTS notifications (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    -- The person receiving the notification
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE, 
    -- The person who triggered the action
    actor_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE, 
    
    type notification_type NOT NULL,
    
    -- Contextual references (nullable depending on the type)
    post_id UUID REFERENCES posts(id) ON DELETE CASCADE,
    comment_id UUID REFERENCES comments(id) ON DELETE CASCADE,
    
    is_read BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Prevent an actor from spamming the exact same notification 
    -- (e.g., liking/unliking rapidly)
    UNIQUE(user_id, actor_id, type, post_id, comment_id) 
);

-- Index for fetching a user's feed quickly
CREATE INDEX IF NOT EXISTS idx_notifications_user_id_created ON notifications(user_id, created_at DESC);
-- Index for finding unread counts
CREATE INDEX IF NOT EXISTS idx_notifications_unread ON notifications(user_id) WHERE is_read = FALSE;