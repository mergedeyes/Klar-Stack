-- UUIDv7 compatibility function.
--
-- Postgres 18 added a native uuidv7() builtin, but rather than depend on
-- that (and risk breaking on an older Postgres, or on a managed provider
-- that hasn't backported it), we define our own — no extensions required,
-- built entirely from core functions (gen_random_uuid, bytea ops).
--
-- Why v7 over the v4 used everywhere before: v4 is fully random, which
-- scatters B-tree index inserts across pages (poor cache locality, more
-- write amplification as tables grow into the millions of rows). v7 is
-- time-ordered in its high bits while still carrying ~74 random bits, so
-- it keeps the "can't be enumerated/guessed" property of v4 while giving
-- roughly sequential index insertion — the same practical benefit as an
-- auto-increment ID, without exposing row counts or ordering to clients.
CREATE OR REPLACE FUNCTION uuid_generate_v7() RETURNS uuid AS $$
DECLARE
    ts_millis   bigint := floor(extract(epoch FROM clock_timestamp()) * 1000)::bigint;
    ts_bytes    bytea  := substring(int8send(ts_millis) FROM 3 FOR 6); -- 48-bit big-endian ms timestamp
    rand_source bytea  := uuid_send(gen_random_uuid()); -- reuse core RNG for the random tail
    result      bytea;
BEGIN
    result := ts_bytes || substring(rand_source FROM 7 FOR 10);
    result := set_byte(result, 6, (get_byte(result, 6) & 15) | 112); -- version nibble = 7
    result := set_byte(result, 8, (get_byte(result, 8) & 63) | 128); -- variant bits = 10
    RETURN encode(result, 'hex')::uuid;
END;
$$ LANGUAGE plpgsql VOLATILE;
