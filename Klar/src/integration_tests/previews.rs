//! The permanent preview-image address used by link previews.

use axum::http::{header, StatusCode};
use serde_json::json;
use sqlx::PgPool;

use super::support::*;

#[sqlx::test(migrations = "./migrations")]
async fn public_post_redirects_to_its_image(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let alice = app.register("alice").await;
    let post = app.upload(&alice, "Sunset").await;

    let res = app.anon_get(&format!("/posts/{post}/preview-image")).await;
    assert_eq!(res.status, StatusCode::FOUND);
    let location = res.headers[header::LOCATION].to_str().unwrap();
    assert!(location.contains("/medium/"), "{location}");
    assert_eq!(res.headers[header::CACHE_CONTROL], "public, max-age=3600");
}

#[sqlx::test(migrations = "./migrations")]
async fn no_image_for_private_warned_or_deleted_posts(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let alice = app.register("alice").await;
    let bob = app.register("bob").await;

    let private = app.upload(&bob, "Private").await;
    app.patch(&bob, "/users/me", json!({ "is_private": true })).await.ok();

    let warned = app.upload(&alice, "Reported").await;
    app.report(&bob, "post", warned, "violence").await;

    let deleted = app.upload(&alice, "Gone").await;
    app.delete(&alice, &format!("/posts/{deleted}")).await.ok();

    for post in [private, warned, deleted] {
        let res = app.anon_get(&format!("/posts/{post}/preview-image")).await;
        assert_eq!(res.status, StatusCode::NOT_FOUND, "post {post}");
    }
}
