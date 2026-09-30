# Klar

Klar is a photo-sharing social network with a **strictly chronological feed**: no algorithm, no ranking, no ads, no tracking. It is built and run from Germany under EU law, and treats privacy and legal compliance as part of the product rather than an afterthought.

Klar is **pre-launch**. The live site ([klarsocial.eu](https://www.klarsocial.eu)) sits behind a passcode while a small group of friends and family test it.

**Contents:** [Principles](#principles) · [Features](#features) · [Safety and moderation](#safety-and-moderation) · [Privacy and your data](#privacy-and-your-data) · [Architecture](#architecture) · [Development](#development) · [Deployment](#deployment) · [Not built yet](#not-built-yet) · [License](#license)

## Principles

- **Chronological, always.** The feed shows posts from people you follow, newest first. Nothing is boosted, hidden or reordered.
- **No ads, no tracking.** Only the storage needed to keep you signed in; no analytics or advertising cookies.
- **Your data stays yours.** Download everything in one ZIP, delete your account at any time, with no waiting period.
- **EU hosting.** Servers, database and storage run in Germany (Bunny.net); email goes through an EU provider.

## Features

### Posts and profiles
- Post a photo with a caption; location and camera data (EXIF) are stripped on upload.
- Edit captions, delete posts.
- Profiles with avatar, display name, bio and a post grid that loads more as you scroll.
- Every post has its own page (`/posts/<id>`): opened from the app it appears over the feed, opened from a shared link it gets a full-screen layout.
- Share button on every post: the system share sheet on phones, "copy link" on desktop. Shared links show a preview (author, caption, image) in messengers, for public accounts only.

### Feeds and discovery
- **Home feed:** posts from people you follow, newest first.
- **Discovery:** posts from across Klar.
- **Search** for people by username or display name.

### People
- Follow and unfollow; followers and following lists.
- **Private accounts:** posts are visible only to approved followers. Follow requests can be accepted from the notification, the requester's profile or a dedicated page.
- Block users: they can no longer follow you, like or comment.

### Conversation
- Likes on posts and comments; threaded comments with replies and @mentions; edit and delete your own comments.
- Real-time notifications for likes, comments, follows and follow requests.
- **Direct messages** between people who follow each other: replies, emoji reactions, read receipts, editing and deleting, all in real time.

### Accounts
- Sign-up with email verification, password reset, password change (signs out other sessions).
- Usernames of 3–30 characters (letters, digits, underscores), case-insensitively unique, changeable every 14 days.

## Safety and moderation

Klar follows the EU Digital Services Act (DSA) for handling reports.

- **Reporting:** posts, comments and profiles can be reported for spam, harassment, hate speech, violence, self-harm, sexual content, child sexual abuse material (CSAM), impersonation or other reasons.
- **Automatic first response:** a CSAM report hides the content immediately; violence, self-harm and sexual-content reports put it behind a warning until a moderator has looked.
- **Statements of reasons (DSA Art. 17):** every removal, hide or warning tells the author what happened, on which ground and whether it was automated, in the app and by email. The author can object once within six months, and a person reviews the objection.
- **Report outcomes (DSA Art. 16):** people who report learn whether it led to action.
- **Evidence preservation:** content reported for a likely-illegal reason is copied at once, including every later edit, so deleting it doesn't destroy the evidence. Copies are encrypted, kept apart from everything else, opened only with a logged reason, and deleted when the report is dismissed or six months after the decision.
- **Rights claims:** rightsholders can report copyright or other infringements through a public form, with or without an account, and follow their claim through a private link. An accepted claim hides the post and can be reversed by the uploader's objection.
- **Admin tools:** a review queue for reports, rights claims, objections and preserved evidence; cases open for more than 30 days are flagged.

## Privacy and your data

- **Data export (GDPR Art. 15/20):** a ZIP with everything Klar holds about you as JSON, plus your photos.
- **Account deletion:** immediate. Your posts, comments, likes and messages go with it, including your messages in other people's chats.
- **Legal pages:** Impressum, privacy policy (Datenschutzerklärung), terms of use, and a plain-language Transparency page. Their "Stand" date is set automatically from the last change.
- **Consent:** accepting the terms at sign-up is required and recorded.
- **Sub-processors:** Bunny.net (hosting, CDN, storage, database; Germany), Scaleway (email; EU), Upstash (real-time relay; US, standard contractual clauses).

## Architecture

| Part | Technology |
| --- | --- |
| Backend API | Rust, [axum](https://github.com/tokio-rs/axum) 0.8, [sqlx](https://github.com/launchbadger/sqlx) (`Klar/`) |
| Frontend | Next.js 16, React 19, Tailwind CSS (`klar-web/`) |
| Database | PostgreSQL 18, self-hosted; migrations run on backend startup |
| Media | S3-compatible storage behind a CDN, with signed, expiring URLs |
| Real-time | Server-Sent Events, fanned out across backend instances through Redis pub/sub |
| Email | Scaleway Transactional Email (MailHog locally) |
| Hosting | Bunny.net Magic Containers in Frankfurt |

```
Klar/        Rust backend: handlers in src/handlers/, routes in src/routes.rs, migrations/
klar-web/    Next.js frontend: pages in src/app/, API client in src/lib/api.ts
deploy/      Postgres and backup-sidecar images
tools/       prod-snapshot: an anonymised copy of production data for local work
docs/        manual-checks.md: what to test by hand before merging
```

<details>
<summary><strong>Technical details</strong></summary>

**Auth.** Short-lived JWT access tokens (15 minutes) sent as a Bearer header, since the sites (klarsocial.eu/.de) and the API are on different domains. Refresh tokens are stored hashed and rotated on every use. Passwords use Argon2. Tokens never travel in URLs; the notification stream uses a single-use, 30-second ticket.

**Feeds.** Fan-out on write into `feed_items`, backfilled on follow and cleaned up on unfollow or block. Paging uses a `(created_at, id)` keyset cursor, so posts with the same timestamp are never skipped.

**Media.** Every upload yields three sizes (thumbnail, medium, full). Client-facing URLs are signed and valid for 6–12 hours, so a copied link stops working after an unfollow or a switch to private; every deletion also purges the CDN. Link previews use a permanent `/posts/<id>/preview-image` address that re-checks the post and redirects to a freshly signed URL.

**Data model.** UUIDv7 keys; denormalised counters updated in the same transaction as the change; hash-partitioned likes and notifications; a monthly-partitioned interaction log (`post_events`, not used for ranking).

**Evidence.** A separate storage zone without a public URL; files encrypted with AES-256-GCM, the key only in the backend's environment; every access and change in an append-only audit log; an hourly sweeper retries copies and purges expired records.

**Operations.** Daily database backups to a private storage zone, caught up after missed slots, with dead-man's-switch alerting. Per-route rate limits, stricter on sign-in and public forms. A health-check endpoint.

</details>

## Development

**You need:** Rust (1.97, as in CI), Node.js 20, PostgreSQL, Redis and [MailHog](https://github.com/mailhog/MailHog) running locally, and a `Klar/.env` (the variables are listed in `Klar/Dockerfile`; set `STORAGE_PROVIDER=local` to store files on disk).

**Run everything:**

```sh
cd klar-web
npm install
npm run dev     # MailHog, the backend on :3000 (runs migrations first) and the frontend on :3001
```

**Realistic data:** `tools/prod-snapshot/` builds an anonymised copy of the production database: structure and volumes as in production, nothing personal. See its [README](tools/prod-snapshot/README.md).

**Checks** (CI runs these on every pull request):

```sh
(cd Klar && export SQLX_OFFLINE=true && cargo clippy --all-targets -- -D warnings && cargo test)
(cd klar-web && npx tsc --noEmit && npx eslint --max-warnings 0 src)
```

**Integration tests** run the API against a real Postgres (a fresh database per test) and Redis, covering moderation, evidence, rights claims, feedback and link previews. CI runs them on every pull request; locally, with both running:

```sh
cd Klar
DATABASE_URL=postgres://postgres:postgres@localhost:5432/postgres REDIS_URL=redis://127.0.0.1:6379 \
SQLX_OFFLINE=true cargo test --features integration-tests integration_tests
```

**Browser tests** (Playwright) cover the page layout, the post modal and page, sharing and link previews; CI runs them against a full stack on every pull request. See [klar-web/e2e](klar-web/e2e/README.md) to run them locally.

After changing a `query!` macro, regenerate the offline cache with `cargo sqlx prepare`. For changes to how the app behaves, go through the matching sections of [docs/manual-checks.md](docs/manual-checks.md); the pull-request template asks for them.

## Deployment

Pushing to `main` builds Docker images for the backend and the frontend and deploys them to Bunny.net; a newer deploy cancels one still running. The backend's settings are environment variables declared in `Klar/Dockerfile` and filled in on Bunny. Besides the usual database, email, Redis and storage settings, the evidence store needs its own storage zone and an encryption key (`EVIDENCE_S3_STORAGE_*`, `EVIDENCE_ENCRYPTION_KEY`).

## Not built yet

- Rights claims for comments and profiles (posts only for now); UrhDaG-specific obligations.
- Handling people who repeatedly file unfounded reports (DSA Art. 23); account suspension from the report queue.
- Age verification at sign-up.
- Uptime monitoring for the app itself (only backups are monitored).
- End-to-end encryption for direct messages.
- Screenshots in feedback; a written guide for testers.

## License

[GNU Affero General Public License v3.0](LICENSE).
