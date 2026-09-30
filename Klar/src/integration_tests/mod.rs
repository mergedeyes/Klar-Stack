//! API-level tests: the real router, called in-process, against a real
//! Postgres (a fresh database per test, migrated by `sqlx::test`) and
//! Redis. They cover the flows where the logic spans handlers, the
//! database and storage: evidence preservation, moderation decisions,
//! rights claims, feedback, link previews and the uptime ping.
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
mod account_review;
mod evidence;
mod feedback;
mod legal_updates;
mod moderation;
mod previews;
mod rights;
mod standing;
mod test_phase;
mod uptime;
