//! API-level tests: the real router, called in-process, against a real
//! Postgres (a fresh database per test, migrated by `sqlx::test`) and
//! Redis. They cover the flows where the logic spans handlers, the
//! database and storage: evidence preservation, moderation decisions,
//! rights claims, feedback, link previews and the uptime ping, and the
//! fixes that must not regress: who can see what (emails, private
//! accounts, hidden content), sessions and email links, follows, feeds,
//! blocks and counters, chats, and the account's export and deletion.
//!
//! Run with services up:
//!
//! ```sh
//! DATABASE_URL=postgres://postgres:postgres@localhost:5432/postgres \
//! REDIS_URL=redis://127.0.0.1:6379 SQLX_OFFLINE=true \
//! cargo test --features integration-tests
//! ```
//!
//! `DATABASE_URL` must point at a server where the user may create
//! databases. CI runs them in the backend job (.github/workflows/ci.yml).

mod support;

mod account_lock;
mod accounts;
mod account_review;
mod audit_export;
mod chats;
mod evidence;
mod feedback;
mod legal_updates;
mod moderation;
mod notices;
mod post_events;
mod official_accounts;
mod previews;
mod privacy;
mod profile_moderation;
mod reports;
mod retention;
mod rights;
mod sessions;
mod social;
mod standing;
mod test_phase;
mod uptime;
