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

    app.delete(&alice, "/users/me").await.ok();

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
