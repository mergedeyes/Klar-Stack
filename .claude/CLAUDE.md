# Klar — notes for Claude

Chronological, no-ranking social network. Solo project, pre-launch, running behind a site passcode.

## Priorities

`TODO.md` (repo root, gitignored — local only) holds the task list. The **BLOCKERS** section at its top (P0 security/privacy, P1 bugs, P2 cleanup) must be worked through before any roadmap item. Check it first when asked "what's next". Never copy vulnerability details into committed files — the repo is public (AGPL).

## Layout

- `Klar/` — Rust backend (axum 0.8, sqlx/Postgres, Redis pub/sub for SSE fan-out across replicas). Handlers in `src/handlers/`, routes in `src/routes.rs`, migrations in `migrations/` (applied on startup).
- `klar-web/` — Next.js 16 frontend (`src/proxy.ts` is the passcode gate; it does NOT protect the API). API client: `src/lib/api.ts`.
- `deploy/` — Postgres + backup sidecar images.

## Checks

- Backend: `cd Klar && SQLX_OFFLINE=true cargo clippy --all-targets --features integration-tests` (after changing a `query!` macro, regenerate `.sqlx/` with `cargo sqlx prepare`).
- Backend integration tests (`Klar/src/integration_tests/`, real Postgres + Redis): `SQLX_OFFLINE=true cargo test --features integration-tests integration_tests` with `DATABASE_URL` (a server where databases may be created) and `REDIS_URL` set. New backend behaviour gets a test there, not a throwaway script.
- Frontend: `cd klar-web && npx tsc --noEmit && npx eslint src`.
- Browser tests (`klar-web/e2e/`, Playwright, needs the stack running): `npm run test:e2e`. UI behaviour worth keeping gets a test there.
- Manual: for behaviour changes, copy the matching sections of `docs/manual-checks.md` into the PR test plan (unticked — the user checks them in a browser). Add a check there when a change introduces behaviour worth re-testing.

## Backend conventions

- Errors: return `AppError`; wrap sqlx results with `.db_err("…")` / `.db_err_ctx(log, client)` from `utils.rs` — never leak raw DB errors to clients.
- Media: DB stores bare storage keys; every response calls `.resolve_media(&state.storage)` right before returning (`ResolveMedia` trait in `utils.rs`).
- Usernames: stored with case preserved; always compare `LOWER(username) = LOWER($1)`.
- Counters (`follower_count`, `post_count`, `like_count`, `comment_count`) are denormalized — update them in the same transaction as the write that changes them.
- Feed is fan-out-on-write into `feed_items`; follow/unfollow/block must backfill/clean it.
- Visibility: private accounts gated via `can_view_posts` (posts.rs); `moderation_status = 'hidden'` content is invisible to everyone but the owner.
- Other users must only ever be serialized as `UserPublicResponse` — `UserResponse` contains the email and is for the account owner only.
- Notifications: insert inside the tx, `publish_notification` to Redis only after commit.
- Auth: short-lived JWT (15 min) via Bearer header (cross-site domains klarsocial.eu/.de → api.klarsocial.eu); refresh tokens stored hashed and rotated on use. Never accept tokens from the query string (URLs land in CDN logs) — the SSE stream uses a single-use 30s ticket from `POST /notifications/stream-ticket`.

## Compliance

A change to the Terms (`nutzungsbedingungen`) or the privacy page (`datenschutz`) needs a notice for existing users in `klar-web/legal-updates/` in the same PR: `documents: terms, privacy`, a `---` line, then a short summary in plain German (format in `.github/scripts/legal_notices.py`). CI refuses the change without one, unless the PR has the label `legal: no notice` (typo fixes). The frontend deploy publishes each file once, after the new page is live.

Klar is an EU service run from Germany, so always consider the GDPR (DSGVO) and German/EU law (DSA, TDDDG, StGB §184b for CSAM) when designing a change. Check data minimisation, retention and deletion, sub-processors (update the Datenschutz page when one is added), evidence preservation for illegal content, and whether the action leaves an audit trail (who, what, when, why). Flag legal questions for review instead of guessing.

## Style

Comments explain *why* in full sentences; match the surrounding comment density. Some older comments are German — fine to leave.
