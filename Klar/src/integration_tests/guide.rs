//! The welcome guide for new accounts (users::mark_guide_seen): shown until
//! it's finished or closed, once per account.

use serde_json::json;
use sqlx::PgPool;

use super::support::*;

#[sqlx::test(migrations = "./migrations")]
async fn a_new_account_sees_the_guide_until_it_is_closed(pool: PgPool) {
    let app = TestApp::new(pool).await;
    // Unverified on purpose: the guide comes right after sign-up, before
    // the email link is clicked.
    let newcomer = app.register_unverified("newcomer").await;
    let other = app.register("other").await;

    assert_eq!(app.get(&newcomer, "/users/me").await.ok().json()["guide_seen"], false);
    app.post(&newcomer, "/users/me/guide-seen", json!({})).await.ok();
    assert_eq!(app.get(&newcomer, "/users/me").await.ok().json()["guide_seen"], true);

    let first = app.export(&newcomer).await["profile"]["guide_seen_at"].as_str().unwrap().to_string();
    app.post(&newcomer, "/users/me/guide-seen", json!({})).await.ok();
    assert_eq!(app.export(&newcomer).await["profile"]["guide_seen_at"], first.as_str(), "closing it again keeps the first date");

    // Only the owner sees the flag.
    assert!(app.get(&other, "/users/newcomer").await.ok().json().get("guide_seen").is_none());
}
