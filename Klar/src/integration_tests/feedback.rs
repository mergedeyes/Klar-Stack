//! In-app feedback (handlers/feedback.rs).

use axum::http::{header, StatusCode};
use serde_json::json;
use sqlx::PgPool;

use super::support::*;
use crate::handlers::feedback::{delete_expired, delete_expired_screenshots};

#[sqlx::test(migrations = "./migrations")]
async fn feedback_is_validated_and_context_is_optional(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let tester = app.register("tester").await;

    assert_eq!(app.anon_post("/feedback", json!({ "category": "bug", "message": "hello there" })).await.status, StatusCode::UNAUTHORIZED);
    let long = "x".repeat(4001);
    for (body, why) in [
        (vec![("category", "rant"), ("message", "hello there")], "bad category"),
        (vec![("category", "bug"), ("message", "hi")], "too short"),
        (vec![("category", "bug"), ("message", long.as_str())], "too long"),
    ] {
        assert_eq!(app.feedback(&tester, &body, vec![]).await.status, StatusCode::BAD_REQUEST, "{why}");
    }

    app.feedback(&tester, &[
        ("category", "bug"), ("message", "Feed doesn't load"),
        ("page_path", "/reset-password?token=SECRET#x"), ("user_agent", "UA/1.0"), ("viewport", "390×844"),
    ], vec![]).await.ok();
    assert_eq!(app.scalar("SELECT page_path FROM feedback WHERE category = 'bug'").await.as_deref(), Some("/reset-password"));
    assert_eq!(app.count("SELECT 1 FROM feedback WHERE page_path || user_agent || message LIKE '%SECRET%'").await, 0);

    app.feedback(&tester, &[("category", "idea"), ("message", "Dark mode toggle please")], vec![]).await.ok();
    assert_eq!(
        app.scalar("SELECT page_path IS NULL AND user_agent IS NULL AND viewport IS NULL FROM feedback WHERE category = 'idea'").await.as_deref(),
        Some("true")
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn admins_triage_feedback(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (tester, admin) = (app.register("tester").await, app.admin().await);
    app.feedback(&tester, &[("category", "bug"), ("message", "Something broke")], vec![]).await.ok();

    assert_eq!(app.get(&tester, "/admin/feedback").await.status, StatusCode::FORBIDDEN);
    let open = app.get(&admin, "/admin/feedback").await.ok().json();
    assert_eq!(open[0]["username"], "tester");
    let id = open[0]["id"].as_str().unwrap().to_string();

    assert_eq!(app.patch(&admin, &format!("/admin/feedback/{id}"), json!({ "status": "wontfix" })).await.status, StatusCode::BAD_REQUEST);
    app.patch(&admin, &format!("/admin/feedback/{id}"), json!({ "status": "done", "admin_note": "fixed in #50" })).await.ok();
    assert!(app.get(&admin, "/admin/feedback").await.ok().json().as_array().unwrap().is_empty(), "done is hidden from the open list");
    let all = app.get(&admin, "/admin/feedback?filter=all").await.ok().json();
    assert_eq!(all[0]["admin_note"], "fixed in #50");
}

#[sqlx::test(migrations = "./migrations")]
async fn daily_cap_export_deletion_and_retention(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let tester = app.register("tester").await;
    app.feedback(&tester, &[("category", "other"), ("message", "First message")], vec![]).await.ok();
    app.exec(&format!(
        "INSERT INTO feedback (user_id, category, message) SELECT '{}', 'other', 'filler' FROM generate_series(1, 19)",
        tester.id
    )).await;
    assert_eq!(app.feedback(&tester, &[("category", "bug"), ("message", "One more please")], vec![]).await.status, StatusCode::BAD_REQUEST);

    assert_eq!(app.export(&tester).await["feedback_sent"].as_array().unwrap().len(), 20);

    app.delete(&tester, "/users/me").await.ok();
    assert_eq!(app.count("SELECT 1 FROM feedback WHERE message = 'First message' AND user_id IS NULL").await, 1, "text stays, link goes");

    app.exec("UPDATE feedback SET created_at = NOW() - INTERVAL '366 days' WHERE message = 'filler'").await;
    assert_eq!(delete_expired(&app.state.db).await.unwrap(), 19);
    assert_eq!(app.count("SELECT 1 FROM feedback").await, 1);
}

#[sqlx::test(migrations = "./migrations")]
async fn screenshots_are_admin_only_exported_and_expire(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (tester, other, admin) = (app.register("tester").await, app.register("other").await, app.admin().await);
    let text = [("category", "bug"), ("message", "Footer covers the button")];

    let four = (0..4).map(|_| png(300, 600, [10, 20, 30])).collect();
    assert_eq!(app.feedback(&tester, &text, four).await.status, StatusCode::BAD_REQUEST, "at most three");
    assert_eq!(app.feedback(&tester, &text, vec![b"not an image".to_vec()]).await.status, StatusCode::BAD_REQUEST);
    assert_eq!(app.count("SELECT 1 FROM feedback").await, 0, "a rejected upload saves nothing");

    // A wide desktop screenshot is scaled down to 1600 px, re-encoded as WebP.
    app.feedback(&tester, &text, vec![png(2000, 1000, [200, 0, 0]), png(300, 600, [0, 200, 0])]).await.ok();
    let list = app.get(&admin, "/admin/feedback").await.ok().json();
    let shots = list[0]["screenshots"].as_array().unwrap().clone();
    assert_eq!(shots.len(), 2);
    assert_eq!((shots[0]["width"].as_i64(), shots[0]["height"].as_i64()), (Some(1600), Some(800)));

    let url = format!("/admin/feedback/screenshots/{}", shots[0]["id"].as_str().unwrap());
    assert_eq!(app.get(&tester, &url).await.status, StatusCode::FORBIDDEN, "not even the sender, through this route");
    let image = app.get(&admin, &url).await.ok();
    assert_eq!(image.headers[header::CONTENT_TYPE], "image/webp");
    assert_eq!(&image.body[8..12], b"WEBP");

    // The sender's export carries the files.
    let res = app.get(&tester, "/users/me/export").await.ok();
    let zip = zip::ZipArchive::new(std::io::Cursor::new(res.body.to_vec())).unwrap();
    assert_eq!(zip.file_names().filter(|n| n.starts_with("feedback/")).count(), 2);
    assert_eq!(app.export(&tester).await["feedback_sent"][0]["screenshots"].as_array().unwrap().len(), 2);

    // Retention: kept while open; 30 days after "done" they go, text stays.
    let id = list[0]["id"].as_str().unwrap().to_string();
    let keys: Vec<String> = sqlx::query_scalar("SELECT storage_key FROM feedback_screenshots").fetch_all(&app.state.db).await.unwrap();
    assert!(keys.iter().all(|k| app.media_file(k).exists()));
    assert_eq!(delete_expired_screenshots(&app.state).await.unwrap(), 0);
    app.patch(&admin, &format!("/admin/feedback/{id}"), json!({ "status": "done" })).await.ok();
    app.exec("UPDATE feedback SET done_at = NOW() - INTERVAL '31 days'").await;
    assert_eq!(delete_expired_screenshots(&app.state).await.unwrap(), 2);
    assert!(keys.iter().all(|k| !app.media_file(k).exists()));
    assert_eq!(app.count("SELECT 1 FROM feedback").await, 1);

    // 90 days after sending at the latest, even while still open.
    app.feedback(&other, &text, vec![png(300, 600, [0, 0, 200])]).await.ok();
    app.exec("UPDATE feedback_screenshots SET created_at = NOW() - INTERVAL '91 days'").await;
    assert_eq!(delete_expired_screenshots(&app.state).await.unwrap(), 1);

    // Deleting the account takes the screenshots along at once.
    app.feedback(&other, &text, vec![png(300, 600, [0, 0, 200])]).await.ok();
    let key: String = sqlx::query_scalar("SELECT storage_key FROM feedback_screenshots").fetch_one(&app.state.db).await.unwrap();
    app.delete(&other, "/users/me").await.ok();
    assert_eq!(app.count("SELECT 1 FROM feedback_screenshots").await, 0);
    assert!(!app.media_file(&key).exists());
}
