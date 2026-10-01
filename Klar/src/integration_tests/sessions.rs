//! Sign-in, sessions and email links (handlers/auth.rs, notifications.rs):
//! refresh tokens are single-use, tokens never come from a URL, stream
//! tickets work once, and changing or resetting the password ends every
//! other session.

use axum::http::StatusCode;
use serde_json::{json, Value};
use sqlx::PgPool;

use super::support::*;

async fn login(app: &TestApp, email: &str, password: &str) -> Resp {
    app.anon_post("/auth/login", json!({ "email": email, "password": password })).await
}

async fn refresh(app: &TestApp, token: &str) -> Resp {
    app.anon_post("/auth/refresh", json!({ "refresh_token": token })).await
}

#[sqlx::test(migrations = "./migrations")]
async fn refresh_tokens_work_once(pool: PgPool) {
    let app = TestApp::new(pool).await;
    app.register("alice").await;
    let first = login(&app, "alice@example.test", PASSWORD).await.ok().json();
    let token = first["refresh_token"].as_str().unwrap();

    let rotated = refresh(&app, token).await.ok().json();
    assert_ne!(rotated["refresh_token"], first["refresh_token"]);
    assert_eq!(refresh(&app, token).await.status, StatusCode::UNAUTHORIZED, "a used token is gone");
    refresh(&app, rotated["refresh_token"].as_str().unwrap()).await.ok();

    // Two refreshes racing with the same token: exactly one wins, so a
    // stolen copy can't quietly keep a second session alive.
    let session = login(&app, "alice@example.test", PASSWORD).await.ok().json();
    let token = session["refresh_token"].as_str().unwrap();
    let (a, b) = tokio::join!(refresh(&app, token), refresh(&app, token));
    let wins = [a.status, b.status].iter().filter(|s| s.is_success()).count();
    assert_eq!(wins, 1, "{} / {}", a.status, b.status);
}

#[sqlx::test(migrations = "./migrations")]
async fn tokens_never_come_from_the_url(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let alice = app.register("alice").await;
    app.get(&alice, "/users/me").await.ok();
    // URLs end up in CDN and proxy logs, so a token there is never accepted.
    assert_eq!(
        app.anon_get(&format!("/users/me?token={}", alice.token)).await.status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        app.anon_get_status(&format!("/notifications/stream?token={}", alice.token)).await,
        StatusCode::BAD_REQUEST,
        "the stream wants a ticket, not a token"
    );

    // A stream ticket is single-use.
    let ticket = app.post(&alice, "/notifications/stream-ticket", json!({})).await.ok().json()["ticket"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(app.anon_get_status(&format!("/notifications/stream?ticket={ticket}")).await, StatusCode::OK);
    assert_eq!(app.anon_get_status(&format!("/notifications/stream?ticket={ticket}")).await, StatusCode::UNAUTHORIZED);
    assert_eq!(app.anon_get_status("/notifications/stream?ticket=made-up").await, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "./migrations")]
async fn emails_are_matched_case_insensitively(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let body = |username: &str, email: &str| {
        json!({ "username": username, "email": email, "password": PASSWORD, "accept_terms": true })
    };
    let first = app.anon_post("/auth/register", body("mixed", " Mixed.Case@Example.Test ")).await.ok().json();
    assert_eq!(first["user"]["email"], "mixed.case@example.test", "stored trimmed and lowercased");
    assert_eq!(
        app.anon_post("/auth/register", body("other", "mixed.case@EXAMPLE.test")).await.status,
        StatusCode::CONFLICT
    );
    login(&app, "MIXED.CASE@example.TEST", PASSWORD).await.ok();
    // Usernames keep their case but are unique regardless of it.
    assert_eq!(app.anon_post("/auth/register", body("MIXED", "x@example.test")).await.status, StatusCode::CONFLICT);
    assert_eq!(app.anon_get("/users/MiXeD").await.ok().json()["username"], "mixed");
}

#[sqlx::test(migrations = "./migrations")]
async fn changing_or_resetting_the_password_ends_other_sessions(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let alice = app.register("alice").await;
    let laptop = login(&app, "alice@example.test", PASSWORD).await.ok().json();

    // Changing it needs the current password and ends every session.
    let change = |current: &str, new: &str| json!({ "current_password": current, "new_password": new });
    assert_eq!(
        app.patch(&alice, "/users/me/password", change("wrong-password-1", "another-password-2")).await.status,
        StatusCode::BAD_REQUEST
    );
    app.patch(&alice, "/users/me/password", change(PASSWORD, "another-password-2")).await.ok();
    assert_eq!(refresh(&app, laptop["refresh_token"].as_str().unwrap()).await.status, StatusCode::UNAUTHORIZED);
    assert_eq!(login(&app, "alice@example.test", PASSWORD).await.status, StatusCode::BAD_REQUEST);
    let phone = login(&app, "alice@example.test", "another-password-2").await.ok().json();

    // A reset link: the answer doesn't say whether the address exists, the
    // link works once, and it ends every session too.
    let unknown = app.anon_post("/auth/forgot-password", json!({ "email": "nobody@example.test" })).await.ok().json();
    let known = app.anon_post("/auth/forgot-password", json!({ "email": "alice@example.test" })).await.ok().json();
    assert_eq!(unknown, known);
    let token = reset_token(&app, &alice).await;
    let reset = |token: &str| json!({ "token": token, "new_password": "third-password-3" });
    app.anon_post("/auth/reset-password", reset(&token)).await.ok();
    assert_eq!(app.anon_post("/auth/reset-password", reset(&token)).await.status, StatusCode::BAD_REQUEST);
    assert_eq!(refresh(&app, phone["refresh_token"].as_str().unwrap()).await.status, StatusCode::UNAUTHORIZED);
    login(&app, "alice@example.test", "third-password-3").await.ok();

    // Asking again invalidates the earlier link.
    app.anon_post("/auth/forgot-password", json!({ "email": "alice@example.test" })).await.ok();
    let older = reset_token(&app, &alice).await;
    app.anon_post("/auth/forgot-password", json!({ "email": "alice@example.test" })).await.ok();
    assert_eq!(app.anon_post("/auth/reset-password", reset(&older)).await.status, StatusCode::BAD_REQUEST);
}

async fn reset_token(app: &TestApp, user: &User) -> String {
    app.scalar(&format!(
        "SELECT token FROM email_tokens WHERE user_id = '{}' AND token_type = 'password_reset' AND used_at IS NULL \
         ORDER BY created_at DESC LIMIT 1",
        user.id
    ))
    .await
    .expect("a reset token")
}

#[sqlx::test(migrations = "./migrations")]
async fn verification_links_work_once_and_resending_reveals_nothing(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let alice = app.register("alice").await;
    assert_eq!(app.get(&alice, "/users/me").await.ok().json()["email_verified"], false);

    let token = app
        .scalar(&format!("SELECT token FROM email_tokens WHERE user_id = '{}' AND token_type = 'verification'", alice.id))
        .await
        .unwrap();
    app.anon_get(&format!("/auth/verify?token={token}")).await.ok();
    assert_eq!(app.anon_get(&format!("/auth/verify?token={token}")).await.status, StatusCode::BAD_REQUEST);
    assert_eq!(app.get(&alice, "/users/me").await.ok().json()["email_verified"], true);

    // The same answer for an unknown, a verified and an unverified address.
    app.register("bob").await;
    let answers: Vec<Value> = resend_answers(&app, &["nobody@example.test", "alice@example.test", "bob@example.test"]).await;
    assert!(answers.windows(2).all(|w| w[0] == w[1]), "{answers:?}");
}

async fn resend_answers(app: &TestApp, emails: &[&str]) -> Vec<Value> {
    let mut out = Vec::new();
    for email in emails {
        out.push(app.anon_post("/auth/resend-verification", json!({ "email": email })).await.ok().json());
    }
    out
}
