//! Locking an account after a suspected takeover (handlers/account_lock.rs).

use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use super::support::*;

async fn login(app: &TestApp, username: &str, password: &str) -> StatusCode {
    app.anon_post("/auth/login", json!({ "email": format!("{username}@example.test"), "password": password }))
        .await
        .status
}

#[sqlx::test(migrations = "./migrations")]
async fn a_locked_account_is_signed_out_and_unlocked_by_a_new_password(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (alice, bob, admin) = (app.register("alice").await, app.register("bob").await, app.admin().await);

    let lock = "/admin/users/alice/lock";
    assert_eq!(app.post(&bob, lock, json!({ "note": "spam" })).await.status, StatusCode::FORBIDDEN);
    assert_eq!(app.post(&admin, lock, json!({ "note": " " })).await.status, StatusCode::BAD_REQUEST);
    app.post(&admin, lock, json!({ "note": "Posted 40 crypto links in 5 minutes, never did before" })).await.ok();
    assert_eq!(app.post(&admin, lock, json!({ "note": "again" })).await.status, StatusCode::CONFLICT);

    // Signed out everywhere, at once.
    assert_eq!(app.get(&alice, "/users/me").await.status, StatusCode::UNAUTHORIZED);
    assert_eq!(app.count(&format!("SELECT 1 FROM refresh_tokens WHERE user_id = '{}'", alice.id)).await, 0);
    app.get(&bob, "/users/me").await.ok();

    // The lock only shows after the right password.
    assert_eq!(login(&app, "alice", "wrong-password-1").await, StatusCode::BAD_REQUEST);
    assert_eq!(login(&app, "alice", PASSWORD).await, StatusCode::LOCKED);

    // Resending: needs the password, and not more than every 15 minutes.
    let resend = |password: &'static str| {
        app.anon_post("/auth/locked/resend-link", json!({ "email": "alice@example.test", "password": password }))
    };
    assert_eq!(resend("wrong-password-1").await.status, StatusCode::BAD_REQUEST);
    assert_eq!(resend(PASSWORD).await.status, StatusCode::TOO_MANY_REQUESTS);
    app.exec("UPDATE account_locks SET last_link_sent_at = NOW() - INTERVAL '16 minutes'").await;
    resend(PASSWORD).await.ok();
    assert_eq!(app.scalar("SELECT links_sent FROM account_locks").await.as_deref(), Some("2"));
    assert_eq!(
        app.count(&format!("SELECT 1 FROM email_tokens WHERE user_id = '{}' AND token_type = 'password_reset' AND expires_at > NOW() + INTERVAL '23 hours'", alice.id)).await,
        2,
        "the lock email's link and the resent one, each valid 24 hours"
    );

    // A new password through the link unlocks the account.
    let token = app
        .scalar(&format!("SELECT token FROM email_tokens WHERE user_id = '{}' AND token_type = 'password_reset' ORDER BY expires_at DESC LIMIT 1", alice.id))
        .await
        .unwrap();
    app.anon_post("/auth/reset-password", json!({ "token": token, "new_password": "a-brand-new-password-9" })).await.ok();
    assert_eq!(login(&app, "alice", "a-brand-new-password-9").await, StatusCode::OK);
    assert_eq!(app.scalar("SELECT unlocked_via FROM account_locks").await.as_deref(), Some("password_reset"));
    assert_eq!(resend(PASSWORD).await.status, StatusCode::BAD_REQUEST, "the old password is gone");
}

#[sqlx::test(migrations = "./migrations")]
async fn locks_are_the_incident_log(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (alice, admin) = (app.register("alice").await, app.admin().await);
    assert_eq!(app.post(&admin, "/admin/users/site_admin/lock", json!({ "note": "x" })).await.status, StatusCode::BAD_REQUEST);

    app.post(&admin, "/admin/users/alice/lock", json!({ "note": "Spam burst" })).await.ok();
    let locks = app.get(&admin, "/admin/locks").await.ok().json();
    assert_eq!(locks[0]["username"], "alice");
    assert_eq!(locks[0]["locked_by"], "site_admin");
    assert!(locks[0]["unlocked_at"].is_null());
    let id = locks[0]["id"].as_str().unwrap().to_string();
    assert_eq!(app.get(&alice, "/admin/locks").await.status, StatusCode::UNAUTHORIZED, "alice is signed out");

    app.patch(&admin, &format!("/admin/locks/{id}"), json!({ "assessment": "Only public posts; low risk, not reported." })).await.ok();
    app.post(&admin, &format!("/admin/locks/{id}/unlock"), json!({})).await.ok();
    assert_eq!(app.post(&admin, &format!("/admin/locks/{id}/unlock"), json!({})).await.status, StatusCode::CONFLICT);
    assert_eq!(login(&app, "alice", PASSWORD).await, StatusCode::OK);

    // The record outlives the account.
    let alice = app.anon_post("/auth/login", json!({ "email": "alice@example.test", "password": PASSWORD })).await.ok().json();
    let token = alice["access_token"].as_str().unwrap();
    let res = app
        .send(
            axum::http::Request::builder()
                .method("DELETE")
                .uri("/users/me")
                .header("authorization", format!("Bearer {token}"))
                .body(axum::body::Body::empty())
                .unwrap(),
            "10.250.0.1",
        )
        .await;
    assert!(res.status.is_success());
    let locks = app.get(&admin, "/admin/locks").await.ok().json();
    assert!(locks[0]["username"].is_null());
    assert_eq!(locks[0]["assessment"], "Only public posts; low risk, not reported.");
}
