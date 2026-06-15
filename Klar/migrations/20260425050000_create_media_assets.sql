-- Media assets — images attached to posts
-- Separate table because a post can have multiple images (carousel, later)
-- For now we'll do one image per post but the schema supports more
CREATE TABLE media_assets (
    id          UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    post_id     UUID NOT NULL REFERENCES posts(id) ON DELETE CASCADE,
    -- Storage paths for each variant (relative to upload dir)
    original_key TEXT NOT NULL,
    thumb_key    TEXT NOT NULL,
    medium_key   TEXT NOT NULL,
    full_key     TEXT NOT NULL,
    -- Original image dimensions
    width       INTEGER NOT NULL,
    height      INTEGER NOT NULL,
    -- File size of the original in bytes
    size_bytes  BIGINT NOT NULL,
    -- Display order for carousels
    sort_order  INTEGER NOT NULL DEFAULT 0,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_media_assets_post_id ON media_assets (post_id);
