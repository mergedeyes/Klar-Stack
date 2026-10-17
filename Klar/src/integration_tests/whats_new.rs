//! "What's new" (users::mark_whats_new_seen): GET /users/me reports the
//! newest release an account has seen, or its sign-up date before the
//! first, so nobody is shown news from before they joined.

use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use super::support::*;

async fn seen(app: &TestApp, user: &User) -> String {
    app.get(user, "/users/me").await.ok().json()["whats_new_seen"].as_str().unwrap().to_string()
}

#[sqlx::test(migrations = "./migrations")]
async fn the_newest_release_seen_is_kept_per_account(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let alice = app.register("alice").await;
    let other = app.register("other").await;

    // Before any: the sign-up day.
    app.exec(&format!("UPDATE users SET created_at = '2026-09-01T12:00:00Z' WHERE id = '{}'", alice.id)).await;
    assert_eq!(seen(&app, &alice).await, "2026-09-01");

    app.post(&alice, "/users/me/whats-new-seen", json!({ "id": "2026-10-18" })).await.ok();
    assert_eq!(seen(&app, &alice).await, "2026-10-18");
    // A tab left open since an older release doesn't take it back.
    app.post(&alice, "/users/me/whats-new-seen", json!({ "id": "2026-10-01" })).await.ok();
    assert_eq!(seen(&app, &alice).await, "2026-10-18");
    // A second release on the same day sorts after the first.
    app.post(&alice, "/users/me/whats-new-seen", json!({ "id": "2026-10-18-b" })).await.ok();
    assert_eq!(seen(&app, &alice).await, "2026-10-18-b");
    assert_eq!(app.export(&alice).await["profile"]["whats_new_seen"], "2026-10-18-b");

    for bad in ["18.10.2026", "2026-13-01", "2026-10-18-", "2026-10-18-B", "2026-10-18x", "x"] {
        let res = app.post(&alice, "/users/me/whats-new-seen", json!({ "id": bad })).await;
        assert_eq!(res.status, StatusCode::BAD_REQUEST, "{bad}");
    }
    assert!(app.get(&other, "/users/alice").await.ok().json().get("whats_new_seen").is_none(), "only the owner sees it");
}
