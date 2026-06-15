-- Add password_hash to users table
-- Nullable for now because the existing "jan" row doesn't have one.
-- In production you'd force a password reset for existing users.
ALTER TABLE users ADD COLUMN password_hash TEXT;