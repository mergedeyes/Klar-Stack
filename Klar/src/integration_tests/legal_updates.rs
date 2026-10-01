//! Notices about changed Terms and privacy policy
//! (handlers/legal_updates.rs).

use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use serde_json::{json, Value};
use sqlx::PgPool;

use super::support::*;

async fn pending(app: &TestApp, user: &User) -> Vec<Value> {
    app.get(user, "/legal-updates/pending").await.ok().json().as_array().unwrap().clone()
}

#[sqlx::test(migrations = "./migrations")]
async fn existing_accounts_must_accept_changed_terms(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (alice, bob, admin) = (app.register("alice").await, app.register("bob").await, app.admin().await);

    let publish = |body: Value| app.post(&admin, "/admin/legal-updates", body);
    assert_eq!(app.post(&alice, "/admin/legal-updates", json!({ "documents": ["terms"], "summary": "x".repeat(30) })).await.status, StatusCode::FORBIDDEN);
    assert_eq!(publish(json!({ "documents": [], "summary": "x".repeat(30) })).await.status, StatusCode::BAD_REQUEST);
    assert_eq!(publish(json!({ "documents": ["cookies"], "summary": "x".repeat(30) })).await.status, StatusCode::BAD_REQUEST);
    assert_eq!(publish(json!({ "documents": ["terms"], "summary": "too short" })).await.status, StatusCode::BAD_REQUEST);
    let terms = publish(json!({ "documents": ["terms"], "summary": "New points for violence and sexual content." })).await.ok().json();
    let terms_id = terms["id"].as_str().unwrap().to_string();

    // Created afterwards: accepted the new version at sign-up.
    let carol = app.register("carol").await;
    assert!(pending(&app, &carol).await.is_empty());

    let p = pending(&app, &alice).await;
    assert_eq!(p.len(), 1);
    assert_eq!(p[0]["requires_acceptance"], true);
    assert_eq!(p[0]["summary"], "New points for violence and sexual content.");

    let before = app.scalar(&format!("SELECT terms_accepted_at FROM users WHERE id = '{}'", alice.id)).await;
    app.post(&alice, &format!("/legal-updates/{terms_id}/acknowledge"), json!({})).await.ok();
    app.post(&alice, &format!("/legal-updates/{terms_id}/acknowledge"), json!({})).await.ok();
    assert!(pending(&app, &alice).await.is_empty());
    assert_ne!(app.scalar(&format!("SELECT terms_accepted_at FROM users WHERE id = '{}'", alice.id)).await, before);
    assert_eq!(app.scalar("SELECT accepted::text FROM legal_update_acks").await.as_deref(), Some("true"));
    assert_eq!(pending(&app, &bob).await.len(), 1, "bob hasn't accepted yet");

    // A privacy notice is information only.
    let privacy = publish(json!({ "documents": ["privacy"], "summary": "We now describe account reviews." })).await.ok().json();
    let p = pending(&app, &bob).await;
    assert_eq!(p.len(), 2);
    assert_eq!(p[1]["requires_acceptance"], false);
    app.post(&bob, &format!("/legal-updates/{}/acknowledge", privacy["id"].as_str().unwrap()), json!({})).await.ok();
    assert_eq!(pending(&app, &bob).await.len(), 1);

    let list = app.get(&admin, "/admin/legal-updates").await.ok().json();
    let t = list.as_array().unwrap().iter().find(|u| u["id"] == terms_id.as_str()).unwrap();
    assert_eq!(t["acknowledged"], 1);
    assert_eq!(t["audience"], 3, "alice, bob and the admin existed then");
}

#[sqlx::test(migrations = "./migrations")]
async fn emails_go_once_to_verified_addresses_only(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (_verified, _unverified, admin) =
        (app.register("verified").await, app.register_unverified("unverified").await, app.admin().await);

    let id = app.post(&admin, "/admin/legal-updates", json!({ "documents": ["terms", "privacy"], "summary": "Both documents changed today." }))
        .await
        .ok()
        .json()["id"]
        .as_str()
        .unwrap()
        .to_string();

    // The publish starts a run in the background; run it here as the sweep
    // would, twice, to check nobody is mailed twice.
    crate::handlers::legal_updates::send_emails(&app.state).await;
    crate::handlers::legal_updates::send_emails(&app.state).await;
    eventually("emails finished", || async {
        app.scalar(&format!("SELECT emails_finished_at FROM legal_updates WHERE id = '{id}'")).await.is_some()
    })
    .await;
    let mailed = app.scalar(&format!("SELECT string_agg(u.username, ',' ORDER BY u.username) FROM legal_update_emails e JOIN users u ON u.id = e.user_id WHERE e.update_id = '{id}'")).await;
    assert_eq!(mailed.as_deref(), Some("site_admin,verified"), "the unverified address is left out");
}

async fn deploy(app: &TestApp, token: Option<&str>, body: Value) -> Resp {
    let mut req = Request::builder()
        .method(Method::POST)
        .uri("/internal/legal-updates")
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(token) = token {
        req = req.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    app.send(req.body(Body::from(body.to_string())).unwrap(), "10.200.0.1").await
}

#[sqlx::test(migrations = "./migrations")]
async fn the_deploy_publishes_each_notice_file_once(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let alice = app.register("alice").await;
    let notice = json!({
        "key": "2026-10-01-sicherer-ort.md",
        "documents": ["terms"],
        "summary": "Klar soll ein sicherer Ort für alle sein.",
    });

    assert_eq!(deploy(&app, None, notice.clone()).await.status, StatusCode::UNAUTHORIZED);
    assert_eq!(deploy(&app, Some("wrong"), notice.clone()).await.status, StatusCode::UNAUTHORIZED);
    let bad_key = json!({ "key": "../x.md", "documents": ["terms"], "summary": "Klar soll ein sicherer Ort für alle sein." });
    assert_eq!(deploy(&app, Some(LEGAL_UPDATES_TOKEN), bad_key).await.status, StatusCode::BAD_REQUEST);

    let first = deploy(&app, Some(LEGAL_UPDATES_TOKEN), notice.clone()).await;
    assert_eq!(first.status, StatusCode::CREATED);
    assert_eq!(first.json()["created"], true);
    // Every deploy sends every file again: nothing new.
    let again = deploy(&app, Some(LEGAL_UPDATES_TOKEN), notice).await;
    assert_eq!(again.status, StatusCode::OK);
    assert_eq!(again.json()["created"], false);

    assert_eq!(app.count("SELECT 1 FROM legal_updates").await, 1);
    assert_eq!(app.scalar("SELECT source_key FROM legal_updates").await.as_deref(), Some("2026-10-01-sicherer-ort.md"));
    assert!(app.scalar("SELECT published_by FROM legal_updates").await.is_none());
    let p = pending(&app, &alice).await;
    assert_eq!(p.len(), 1);
    assert_eq!(p[0]["requires_acceptance"], true);
}
