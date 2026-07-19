-- Erstellt einen "Composite Index", der exakt unserem ORDER BY entspricht.
CREATE INDEX IF NOT EXISTS idx_posts_keyset_pagination 
ON posts (created_at DESC, id DESC);