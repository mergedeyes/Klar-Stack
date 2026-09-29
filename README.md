# Klar — Feature List (as of 30 September 2026)

## Accounts & Auth
- Registration (username, email, password) with email verification link; emails are matched case-insensitively
- Banner on every page until the email address is verified
- Login / logout, JWT access tokens (15 min) + refresh tokens (30 days); refresh tokens are stored hashed, single-use and rotated on every refresh, and the frontend refreshes ahead of expiry through one shared request
- Cross-site auth support: tokens in `localStorage` + `Authorization: Bearer`, with httpOnly cookies as a same-site fallback (handles `klarsocial.eu` vs `klarsocial.de` being different top-level domains)
- Password reset (forgot password → email link → reset)
- Resend verification email (email-based, works while logged out)
- Argon2 password hashing
- Change password (requires current password, invalidates all other sessions)
- Account deletion (cascades to posts/comments/messages/media; cleans up stored files)
- Self-service data export (GDPR Art. 15/20) — a ZIP with `data.json` (everything Klar holds about you) plus your full-size images and avatar, so the copy doesn't depend on expiring media links

## Profile
- Username (3–30 characters: letters, digits, underscores; case preserved as entered, case-insensitive uniqueness, reserved names such as `me`/`admin` blocked, 14-day change cooldown)
- Display name, bio, avatar (upload, resized/processed server-side)
- Public profile page: avatar, stats (posts/followers/following), bio, and a post grid that loads more as you scroll
- **Private accounts**: toggle in Settings; when on, posts are hidden from non-followers
- Follow requests for private accounts: request → pending → accept/reject (not instant-follow)
  - Accept/decline available in the notification dropdown *and* directly on the requester's profile page
  - Dedicated `/follow-requests` management page

## Posts & Media
- Create post: caption + optional photo
- Automatic EXIF stripping on upload (location, device info, timestamp removed)
- Three generated sizes per image (thumbnail/medium/full), real width/height captured for correct aspect-ratio rendering
- Media is served through signed, expiring CDN URLs (valid 6–12 h), so a copied link stops working after an unfollow, block or switch to private; every media deletion also purges the CDN cache
- Edit caption, delete post (with real cascade cleanup of media files)
- Post permalink pages (`/posts/[id]`) — shareable, deep-linkable
- Post detail modal: comments, likes, delete (from feed, profile, or permalink)

## Feed
- Personal feed: strictly **chronological**, no algorithm, no ranking — deliberate product stance
- Fan-out-on-write (`feed_items`) for fast reads; backfilled on follow, cleaned up on unfollow
- Discovery feed: global, cross-user
- All feeds page with a `(created_at, id)` keyset cursor and infinite scroll, so posts with identical timestamps are never skipped
- Private-account posts are excluded from the discovery feed and profile grids unless you're the owner or an accepted follower (enforced backend-side in three places: single post view, profile posts list, and the discovery feed itself)

## Social Graph
- Follow / unfollow (instant for public accounts, request-based for private ones)
- Followers / following lists
- Block / unblock (blocked users can't follow, like, or comment on your posts)

## Comments & Likes
- Post likes, comment likes
- Comments with threaded replies
- Edit/delete own comments; post owners can also delete comments on their posts

## Notifications
- Types: post like, comment, comment like, follow, follow request, follow accepted
- Persisted history (`GET /notifications`) + real-time delivery via SSE, authenticated with a single-use, 30-second stream ticket (no tokens in URLs)
- **Cross-replica delivery** via Redis (Upstash) pub/sub — works correctly however many backend instances are running, not just replica-count-1
- Notification previews: actor avatar (follows/requests) or post thumbnail (likes/comments)
- Clicking a notification navigates to the relevant post or profile
- Follow-request notifications have inline Accept/Decline buttons, not just a link

## Direct Messages (Chat)
- 1:1 conversations, gated to mutual followers only
- Send, edit, delete messages; reply-to-message threading
- Emoji reactions (8-emoji picker; can react to your own or the other person's messages)
- Read receipts
- Real-time message + reaction delivery (same Redis/SSE pipeline as notifications)
- Unread-message badge on the Chat icon, correctly clears when actively viewing a conversation (no false positives from your own open chat)
- Conversation list shows the true most-recent activity — a plain message, a reply, or a reaction — with proper phrasing ("Me: ...", "X replied: ...", "X reacted 👍 to your message: ...")
- Avatar in conversation list/chat header links to the other person's profile; rest of the row opens the chat
- When an account is deleted, its messages are erased from the partner's chat too; the partner keeps the conversation with their own messages and a read-only "account deleted" notice

## Search
- User search by username or display name

## Moderation & Reporting
- Report posts, comments, or user accounts (`POST /reports`) with a fixed reason taxonomy: spam, harassment, hate speech, violence/graphic content, self-harm, sexual content, CSAM, impersonation, other
- Auto-moderation on submission: a CSAM report hides the content immediately, zero tolerance, no threshold; violence/self-harm/sexual-content reports flag the content (shown behind an interstitial rather than removed outright — a single report shouldn't have unilateral takedown power)
- Admin review queue (`/admin/reports`), critical reports sorted first: dismiss (restores visibility) or remove content outright, each with an optional internal review note
- Admin access gated via an `ADMIN_EMAILS` allow-list, requiring the matching account's email to also be verified (prevents someone from claiming an admin address by registering it first)
- *Not yet built:* DSA-mandated automated "Statement of Reasons" notification to affected users, appeal/counter-notice flow, UrhDaG rights-holder copyright takedown portal

## Privacy & Compliance
- Impressum, Datenschutzerklärung (privacy policy), Nutzungsbedingungen (ToS), and a plain-language **Transparenz** page explaining data handling in everyday terms
- Legal pages carry a "Stand:" date that the frontend build stamps from the last commit on `main` that changed each page (Europe/Berlin time) — nothing is edited by hand or committed by a bot
- ToS + privacy consent checkbox at registration, enforced server-side (`POST /auth/register` rejects the request with 400 if `accept_terms` isn't `true`, and 422 if it's missing entirely) — not just a client-side gate anymore. Acceptance is timestamped (`users.terms_accepted_at`) and included in the self-service data export
- No tracking/advertising cookies — only functional auth storage
- Documented sub-processors: Bunny.net (hosting/CDN/storage and self-hosted Postgres, German DC), Scaleway (transactional email, EU), Upstash (real-time notification relay, US — SCCs apply)

## Pre-Launch
- Site-wide passcode gate (`/welcome`) via Next.js `proxy.ts` — blocks the whole site except the gate page, legal pages and email-link landing pages until a shared passcode is entered; attempts are rate-limited; disabled automatically if no passcode is configured (e.g. local dev)

## Frontend/UX Details
- Consistent top navigation bar across all primary pages (Feed, Discovery, Search, Chats, Profile) with active-state icon highlighting
- Mobile-correct viewport handling (`dvh` instead of `vh`) so nav bars don't get clipped by mobile browser chrome
- Root URL smart-redirects to Feed (logged in) or Login (logged out)
- Global cursor-pointer fix for all interactive elements

## Backend/Infra
- Rust (Axum) backend, self-hosted PostgreSQL 18 in a Bunny Magic Container (Frankfurt) — TLS enforced even on loopback, connection pool retries through pod-startup races
- Daily automated Postgres backups at a fixed time (`db-backup` sidecar → private Bunny Storage Zone, no CDN pull zone); missed or failed slots are caught up on the next start, and a dead-man's-switch ping (Healthchecks.io protocol, no data sent) raises an alert when a backup fails or the sidecar stops running. Durability is handled by off-pod storage, so the sidecar itself needs no persistent volume
- UUIDv7 primary keys, denormalized counters (follower/following/post/like/comment counts), hash-partitioned `likes`/`notifications`, monthly-partitioned interaction event log (`post_events` — logging foundation for possible future recommendations, not used for ranking today)
- Bunny.net: application hosting (Magic Containers), CDN, S3-compatible object storage
- Redis (Upstash, TLS) for cross-replica pub/sub
- Multi-provider transactional email abstraction (currently live on Scaleway TEM)
- Per-route rate limiting (stricter on auth endpoints)
- GitHub Actions CI/CD: every pull request runs `cargo clippy -D warnings`, `cargo test`, `tsc` and `eslint --max-warnings 0`; separate backend/frontend pipelines build Docker images and auto-deploy to Bunny on push to `main`, cancelling a running deploy when a newer one starts
- Health check endpoint
- Local development: disk storage provider instead of Bunny, and an anonymised production snapshot (`tools/prod-snapshot/`) that keeps the data's structure but replaces everything personal

## Development

- Backend: `cd Klar && SQLX_OFFLINE=true cargo clippy` (regenerate `.sqlx/` with `cargo sqlx prepare` after changing a `query!` macro)
- Frontend: `cd klar-web && npx tsc --noEmit && npx eslint src`
- Before merging a change to how the app behaves, go through the matching sections of [docs/manual-checks.md](docs/manual-checks.md) — the PR template asks for them in the test plan

---
*Not yet built / explicitly deferred:* DSA Statement-of-Reasons user notifications & appeal flow, UrhDaG rights-holder copyright portal, birth-date/16+ age verification enforcement, uptime monitoring/alerting for the app itself (only backups are monitored), notifications for follows-of-a-reply/DM-specific push beyond what's listed above, ClickHouse-based ranking (data collection foundation exists, ranking layer doesn't).
