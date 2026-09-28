# prod-snapshot

An anonymized local copy of the production database, built from the latest
nightly backup (`deploy/postgres-backup`). Use it to answer "what does the
real data look like?" and to try migrations on production-shaped data
before deploying — without touching production and without personal data
on your machine.

## Setup (once)

Needs Docker; `migrate` also needs `cargo sqlx` (`cargo install sqlx-cli`).

```sh
cp tools/prod-snapshot/snapshot.env.example tools/prod-snapshot/snapshot.env
# fill in BUNNY_STORAGE_ZONE and BUNNY_STORAGE_PASSWORD of the backup zone
```

`snapshot.env` is gitignored (`*.env`). Bunny storage zones have no
key/secret pair: the zone name is both bucket and access key id, the zone
password is the secret. The read-only password is enough.

## Use

```sh
tools/prod-snapshot/snapshot.sh refresh     # latest backup -> fresh anonymized snapshot
tools/prod-snapshot/snapshot.sh checks      # run every checks/*.sql
tools/prod-snapshot/snapshot.sh migrate     # apply this checkout's pending migrations
tools/prod-snapshot/snapshot.sh psql        # interactive psql
tools/prod-snapshot/snapshot.sh psql -c 'SELECT COUNT(*) FROM posts'
tools/prod-snapshot/snapshot.sh url         # DATABASE_URL, e.g. to run the backend against it
tools/prod-snapshot/snapshot.sh drop        # delete it
```

Every account's password in the snapshot is `klar-dev-password`, so you
can run the backend against it (`DATABASE_URL=$(… url) cargo run`) and log
in as any user (`user_<n>@example.invalid`).

**Before deploying a PR with migrations:** `refresh`, then `migrate` from
that branch. It applies exactly what the backend would apply on startup and
shows any `WARNING` a migration raised.

## What happens on `refresh`

1. The previous snapshot container is removed.
2. A fresh `postgres:18.6-trixie` (same image as production) starts on
   `127.0.0.1:55432`, with its data directory on **tmpfs**: the snapshot
   lives in RAM only and disappears when the container stops or the
   machine reboots. Nothing is written to disk.
3. The latest `backups/*.dump` is **streamed** from Bunny Storage straight
   into `pg_restore` (the dump file never touches disk) into a database
   called `klar_import`.
4. `preflight.sql` runs on the real data and prints **counts only** —
   for questions that need the original values, e.g. "are there emails
   that differ only in case?".
5. `anonymize.sql` runs in a single transaction: names, emails, bios,
   avatars, captions, comments, messages, report notes and image keys are
   replaced; tokens are deleted. Ids, relations, timestamps, counters and
   moderation states are kept, so the structure matches production.
6. Only then is `klar_import` renamed to `klar`. If any step fails (or you
   press Ctrl-C), the raw import is dropped.

### Adding a column

`anonymize.sql` ends with a guard that aborts if the schema has a text,
enum, JSON or other non-id column it hasn't been told about. After a
migration adds one, `refresh` fails until you decide: anonymize it in
`anonymize.sql`, or add it to `reviewed_columns` if it can't hold personal
data.

### Adding checks

- Needs the original values → add an aggregate (counts only!) to
  `preflight.sql`.
- Works on anonymized data → add a `.sql` file to `checks/`.

## Privacy note

Restoring production data locally is still processing personal data, even
briefly and in RAM. That's why the raw import is anonymized before it gets
its usable name, pre-flight output is limited to counts, and nothing is
persisted. Keep it that way when extending this tool.
