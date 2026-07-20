-- Media assets attached to posts. sort_order supports future carousels
-- (multiple images per post); today the app only shows one image per
-- post, so application queries must filter to sort_order = 0 to avoid
-- row-multiplication once a post legitimately has more than one image
-- (see the Rust handler changes in this same rollout).
CREATE TABLE IF NOT EXISTS media_assets (
    id            UUID PRIMARY KEY DEFAULT uuid_generate_v7(),
    post_id       UUID NOT NULL REFERENCES posts(id) ON DELETE CASCADE,
    original_key  TEXT NOT NULL,
    thumb_key     TEXT NOT NULL,
    medium_key    TEXT NOT NULL,
    full_key      TEXT NOT NULL,
    width         INTEGER NOT NULL,
    height        INTEGER NOT NULL,
    size_bytes    BIGINT NOT NULL,
    sort_order    INTEGER NOT NULL DEFAULT 0,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_media_assets_post_id ON media_assets (post_id, sort_order);
