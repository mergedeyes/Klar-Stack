# ClickHouse — what it is, and how it fits into Klar

## What it is

ClickHouse is a column-oriented database built for analytical queries over
huge, append-only datasets — the opposite shape of problem from Postgres.
Postgres (row-oriented) is great at "fetch this one post and its author" —
a handful of rows, many columns each. ClickHouse is great at "count how many
`like` events happened per hour, per post, over the last 90 days across
50 million rows" — few columns, an enormous number of rows, aggregated.

Concretely: if you tried to run `post_events` (the new interaction log) at
real scale through Postgres for analytical queries -- "which posts got the
most views today", "what's this user's engagement rate this week" -- you'd
either need heavy indexing that slows down writes, or you'd be scanning
huge ranges and competing with your actual transactional traffic (someone
liking a post, loading their feed) for the same database's resources. That
contention is the reason nobody runs analytics and OLTP (the transactional,
read/write app traffic) on the same database at scale.

## Why it matters for ranking/recommendations specifically

Any "best posts to show someone" algorithm needs to answer questions like:
"what's this post's engagement velocity", "which posts has this user
historically engaged with", "what's trending in the last N hours" — all of
which are aggregate queries over the *entire* event history, not lookups
of individual rows. That's exactly ClickHouse's job. Postgres stays the
source of truth for "what exists right now" (posts, users, follows);
ClickHouse becomes the answer to "what happened, and how much of it."

## How data would flow (not yet built — this is the plan, not running infra)

1. `post_events` (built in this rollout) is the source. Every view/like/
   comment/unlike gets a row, partitioned monthly in Postgres.
2. Periodically (e.g. hourly), a small job reads new rows and writes them
   into ClickHouse. Two common ways to do this:
   - **Batch export**: a scheduled job selects rows from the most recent
     `post_events` partition(s) and bulk-inserts them into ClickHouse.
     Simple, good enough as a first step.
   - **CDC (change data capture)**: a tool like Debezium streams every
     insert from Postgres's write-ahead log into ClickHouse continuously,
     no polling needed. More infrastructure, but near real-time.
3. Old, already-exported `post_events` partitions in Postgres can then be
   dropped or archived — this is exactly what the monthly partitioning is
   for: once a month's data is safely in ClickHouse, `DROP TABLE
   post_events_2026_01` is instant, versus a `DELETE` that would have to
   scan and vacuum a huge table.

## A starting ClickHouse schema (reference only)

This is what the corresponding ClickHouse table would look like — written
here as documentation for when you actually stand up a ClickHouse
instance, since I can't provision that infrastructure from this
environment (it's a separate service, not part of this codebase or your
local machine).

```sql
CREATE TABLE post_events
(
    id          UUID,
    user_id     Nullable(UUID),
    post_id     UUID,
    event_type  LowCardinality(String),
    created_at  DateTime64(3),
    event_date  Date MATERIALIZED toDate(created_at)
)
ENGINE = MergeTree
PARTITION BY toYYYYMM(event_date)
ORDER BY (post_id, created_at);

-- Example rollup: engagement per post per day, refreshed incrementally
-- by ClickHouse itself rather than recomputed from raw events each time.
CREATE MATERIALIZED VIEW post_engagement_daily
ENGINE = SummingMergeTree
ORDER BY (post_id, event_date)
AS
SELECT
    post_id,
    event_date,
    countIf(event_type = 'view') AS views,
    countIf(event_type = 'like') AS likes,
    countIf(event_type = 'comment') AS comments
FROM post_events
GROUP BY post_id, event_date;
```

`LowCardinality(String)` and `MergeTree` are ClickHouse-specific — the
former is a compact encoding for columns with few distinct values (like
`event_type`), the latter is ClickHouse's core storage engine, optimized
for exactly this write-once/read-in-bulk pattern.

## What this rollout actually sets up vs. what's still ahead

**Done now:** the Postgres-side event log (`post_events`), designed
specifically to be a clean export source later (immutable rows, no
dedup logic to fight with, natural monthly partitions to export and
drop).

**Not done, and out of scope for what I can do here:** actually
provisioning a ClickHouse instance, building the export job (batch or
CDC), and building anything that reads from it (a ranking service, a
dashboard). Those are infrastructure/ops decisions for when you're
actually ready to build a ranking algorithm — there's no value in
standing up ClickHouse today with nothing feeding it yet beyond what
`post_events` already collects.
