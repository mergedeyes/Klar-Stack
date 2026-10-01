//! The account's own data: the export (Art. 15/20 GDPR) holds every
//! category, with the images as files, and deleting the account removes
//! the content and its files (users.rs).

use std::io::{Cursor, Read};

use serde_json::json;
use sqlx::PgPool;

use super::support::*;

#[sqlx::test(migrations = "./migrations")]
async fn the_export_holds_every_category_with_its_images(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (alice, bob) = (app.register("alice").await, app.register("bob").await);
    app.register("carol").await;
    app.patch(&alice, "/users/me", json!({ "display_name": "Alice A.", "bio": "hello" })).await.ok();
    app.upload_avatar(&alice, [10, 20, 30]).await;
    let post = app.upload(&alice, "my post").await;
    let bobs = app.upload(&bob, "bob's post").await;
    let comment = app.comment(&alice, bobs, "my comment").await;
    app.post(&alice, &format!("/posts/{bobs}/like"), json!({})).await.ok();
    app.post(&alice, &format!("/posts/{bobs}/comments/{comment}/like"), json!({})).await.ok();
    app.post(&alice, "/users/bob/follow", json!({})).await.ok();
    app.post(&bob, "/users/alice/follow", json!({})).await.ok();
    app.post(&alice, "/users/carol/block", json!({})).await.ok();
    app.post(&alice, "/chats/send", json!({ "receiver_id": bob.id, "body": "secret plan" })).await.ok();
    app.post(&bob, &format!("/posts/{post}/like"), json!({})).await.ok();
    app.report(&alice, "post", bobs, "spam").await;
    // Pending follow requests both ways, and an acknowledged notice about
    // the privacy policy.
    let (dave, erin, admin) = (app.register("dave").await, app.register("erin").await, app.admin().await);
    app.patch(&dave, "/users/me", json!({ "is_private": true })).await.ok();
    app.post(&alice, "/users/dave/follow", json!({})).await.ok();
    app.patch(&alice, "/users/me", json!({ "is_private": true })).await.ok();
    app.post(&erin, "/users/alice/follow", json!({})).await.ok();
    let update = app.post(&admin, "/admin/legal-updates", json!({ "documents": ["privacy"], "summary": "Neue Speicherfristen." }))
        .await
        .ok()
        .json();
    app.post(&alice, &format!("/legal-updates/{}/acknowledge", update["id"].as_str().unwrap()), json!({})).await.ok();

    let res = app.get(&alice, "/users/me/export").await.ok();
    assert_eq!(res.headers["content-type"], "application/zip");
    let mut zip = zip::ZipArchive::new(Cursor::new(res.body.to_vec())).unwrap();
    let mut json = String::new();
    zip.by_name("data.json").unwrap().read_to_string(&mut json).unwrap();
    let data: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert_eq!(data["profile"]["email"], "alice@example.test");
    assert_eq!(data["profile"]["display_name"], "Alice A.");
    assert!(!data["profile"]["terms_accepted_at"].is_null());
    assert_eq!(data["posts"].as_array().unwrap().len(), 1);
    assert_eq!(data["comments"][0]["body"], "my comment");
    assert_eq!(data["likes_given"]["posts"].as_array().unwrap().len(), 1);
    assert_eq!(data["likes_given"]["comments"].as_array().unwrap().len(), 1);
    assert_eq!(data["following"][0]["username"], "bob");
    assert_eq!(data["followers"][0]["username"], "bob");
    assert_eq!(data["blocked_users"][0]["username"], "carol");
    assert_eq!(data["follow_requests"]["sent"][0]["username"], "dave");
    assert_eq!(data["follow_requests"]["received"][0]["username"], "erin");
    assert_eq!(data["legal_updates"][0]["documents"], json!(["privacy"]));
    assert_eq!(data["conversations"][0]["messages"][0]["body"], "secret plan");
    assert!(!data["notifications_received"].as_array().unwrap().is_empty(), "bob's follow and like");
    assert_eq!(data["moderation"]["reports_filed"].as_array().unwrap().len(), 1);
    assert_eq!(data["export_info"]["missing_files"], json!([]));

    // Every image is in the archive, at the path data.json names.
    let mut files = vec![data["profile"]["avatar_file"].as_str().unwrap().to_string()];
    for post in data["posts"].as_array().unwrap() {
        for media in post["media"].as_array().unwrap() {
            files.push(media["file"].as_str().unwrap().to_string());
        }
    }
    assert_eq!(files.len(), 2);
    for file in files {
        assert!(zip.by_name(&file).unwrap().size() > 0, "{file} is empty");
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn deleting_the_account_removes_its_content_and_files(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (alice, bob) = (app.register("alice").await, app.register("bob").await);
    app.upload_avatar(&alice, [1, 2, 3]).await;
    let post = app.upload(&alice, "to be deleted").await;
    let bobs = app.upload(&bob, "bob's post").await;
    app.comment(&alice, bobs, "alice was here").await;
    app.report(&alice, "post", bobs, "spam").await;

    let mut keys: Vec<String> = sqlx::query_scalar::<_, String>(
        "SELECT unnest(ARRAY[thumb_key, medium_key, full_key]) FROM media_assets WHERE post_id = $1",
    )
    .bind(post)
    .fetch_all(&app.state.db)
    .await
    .unwrap();
    keys.push(app.scalar(&format!("SELECT avatar_url FROM users WHERE id = '{}'", alice.id)).await.unwrap());
    assert_eq!(keys.len(), 4);
    for key in &keys {
        assert!(app.media_file(key).exists(), "{key} missing before");
    }

    app.delete_account(&alice).await.ok();

    assert_eq!(app.anon_get("/users/alice").await.status.as_u16(), 404);
    assert_eq!(app.get(&bob, &format!("/posts/{post}")).await.status.as_u16(), 404);
    assert_eq!(app.count(&format!("SELECT 1 FROM comments WHERE user_id = '{}'", alice.id)).await, 0);
    for key in &keys {
        assert!(!app.media_file(key).exists(), "{key} left on disk");
    }
    // Her report stays (it's about the content), no longer linked to her.
    assert_eq!(app.count(&format!("SELECT 1 FROM reports WHERE target_id = '{bobs}' AND reporter_id IS NULL")).await, 1);
    // The old session no longer reaches an account.
    assert!(!app.get(&alice, "/users/me").await.status.is_success());
    // The address and the name are free again.
    app.register("alice").await;
}

#[sqlx::test(migrations = "./migrations")]
async fn deleting_the_account_needs_the_password(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let alice = app.register("alice").await;
    // A session alone isn't enough.
    assert!(!app.delete(&alice, "/users/me").await.status.is_success());
    let wrong = app.delete_with(&alice, "/users/me", json!({ "password": "wrong-password-1" })).await;
    assert_eq!(wrong.status, axum::http::StatusCode::BAD_REQUEST);
    app.get(&alice, "/users/me").await.ok();
    app.delete_account(&alice).await.ok();
    assert_eq!(app.count(&format!("SELECT 1 FROM users WHERE id = '{}'", alice.id)).await, 0);
}

#[sqlx::test(migrations = "./migrations")]
async fn the_exports_file_name_reaches_the_frontend(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let alice = app.register("alice").await;
    let req = axum::http::Request::builder()
        .uri("/users/me/export")
        .header("authorization", format!("Bearer {}", alice.token))
        // The default CORS_ORIGINS (routes.rs).
        .header("origin", "http://localhost:5173")
        .body(axum::body::Body::empty())
        .unwrap();
    let res = app.send(req, "10.250.0.2").await.ok();
    let exposed = res.headers["access-control-expose-headers"].to_str().unwrap().to_lowercase();
    assert!(exposed.contains("content-disposition"), "{exposed}");
    assert!(res.headers["content-disposition"].to_str().unwrap().contains("klar-datenexport-"));
}

#[sqlx::test(migrations = "./migrations")]
async fn an_unverified_account_reads_but_neither_publishes_messages_nor_reports(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (alice, bob) = (app.register_unverified("alice").await, app.register("bob").await);
    let post = app.upload(&bob, "bob's post").await;
    app.post(&alice, "/users/bob/follow", json!({})).await.ok();
    app.post(&bob, "/users/alice/follow", json!({})).await.ok();

    let refused = [
        app.try_upload(&alice, "mine", png(50, 50, [1, 2, 3])).await,
        app.post(&alice, "/posts", json!({ "caption": "text only" })).await,
        app.post(&alice, &format!("/posts/{post}/comments"), json!({ "body": "hi" })).await,
        app.post(&alice, "/chats/send", json!({ "receiver_id": bob.id, "body": "hi" })).await,
        app.post(&alice, "/reports", json!({ "target_type": "post", "target_id": post, "reason": "spam" })).await,
    ];
    for res in refused {
        assert_eq!(res.status, axum::http::StatusCode::FORBIDDEN);
        assert!(res.json()["error"].as_str().unwrap().contains("verify your email"));
    }
    // Reading, liking and settings stay open.
    app.get(&alice, &format!("/posts/{post}")).await.ok();
    app.post(&alice, &format!("/posts/{post}/like"), json!({})).await.ok();
    app.patch(&alice, "/users/me", json!({ "bio": "new here" })).await.ok();

    // Verified, it all works.
    let token = app
        .scalar(&format!("SELECT token FROM email_tokens WHERE user_id = '{}' AND token_type = 'verification'", alice.id))
        .await
        .unwrap();
    app.anon_get(&format!("/auth/verify?token={token}")).await.ok();
    app.comment(&alice, post, "hi").await;
}
