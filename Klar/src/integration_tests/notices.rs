//! Notices about illegal content from anyone (handlers/notices.rs, DSA Art.
//! 16): the public form, the report it becomes, the status link and the
//! outcome.

use axum::http::StatusCode;
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use super::support::*;

fn notice(url: &str) -> Value {
    json!({
        "reason": "hate_speech",
        "explanation": "Calls for violence against a religious group (§ 130 StGB).",
        "content_url": url,
        "notifier_name": "Erika Muster",
        "notifier_email": "erika@example.test",
        "good_faith": true,
        "website": ""
    })
}

fn with(mut body: Value, key: &str, value: Value) -> Value {
    body[key] = value;
    body
}

#[sqlx::test(migrations = "./migrations")]
async fn anyone_can_report_illegal_content_and_follow_it(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (alice, admin) = (app.register("alice").await, app.admin().await);
    let post = app.upload(&alice, "Hateful caption").await;
    let url = format!("https://www.klarsocial.eu/posts/{post}");

    for (body, status, why) in [
        (with(notice(&url), "good_faith", json!(false)), StatusCode::BAD_REQUEST, "good faith"),
        (with(notice(&url), "notifier_email", json!(null)), StatusCode::BAD_REQUEST, "contact details"),
        (with(notice(&url), "explanation", json!(" ")), StatusCode::BAD_REQUEST, "explanation"),
        (notice("https://www.klarsocial.eu/feed"), StatusCode::BAD_REQUEST, "not an item"),
        (notice(&format!("https://www.klarsocial.eu/posts/{}", Uuid::new_v4())), StatusCode::NOT_FOUND, "unknown post"),
    ] {
        assert_eq!(app.anon_post("/notices", body).await.status, status, "{why}");
    }
    let bot = app.anon_post("/notices", with(notice(&url), "website", json!("http://spam.example"))).await;
    assert_eq!(bot.status, StatusCode::CREATED, "the honeypot looks accepted");
    assert_eq!(app.count("SELECT 1 FROM content_notices").await, 0, "but nothing is stored");

    // A notice without an account: a report in the queue, with no reporter
    // account, the content preserved, and a status link.
    let created = app.anon_post("/notices", notice(&url)).await.ok().json();
    let (id, token) = (created["id"].as_str().unwrap().to_string(), created["token"].as_str().unwrap().to_string());
    let group = app.get(&admin, "/admin/reports").await.ok().json()[0].clone();
    assert_eq!(group["target_id"], post.to_string());
    assert_eq!(group["reports"][0]["source"], "public_notice");
    assert!(group["reports"][0]["reporter_id"].is_null());
    assert_eq!(group["reports"][0]["notifier_name"], "Erika Muster");
    assert!(!group["evidence_id"].is_null(), "likely illegal: preserved");
    assert_eq!(status(&app, &id, &token).await["outcome"], Value::Null);
    assert_eq!(app.anon_post(&format!("/notices/{id}/status"), json!({ "token": "x".repeat(64) })).await.status, StatusCode::NOT_FOUND);

    // The decision reaches the notifier's status page (and email); the
    // author's statement says it followed a notice.
    let report = group["reports"][0]["id"].as_str().unwrap();
    app.post(&admin, &format!("/admin/reports/{report}/remove"), json!({})).await.ok();
    let st = status(&app, &id, &token).await;
    assert_eq!(st["outcome"], "removed");
    assert!(!st["decided_at"].is_null());
    let d = app.get(&alice, "/moderation/decisions").await.ok().json()[0].clone();
    assert_eq!(d["source"], "notice");
    assert!(!d.to_string().contains("Erika"), "the author doesn't learn who sent it");
}

async fn status(app: &TestApp, id: &str, token: &str) -> Value {
    app.anon_post(&format!("/notices/{id}/status"), json!({ "token": token })).await.ok().json()
}

#[sqlx::test(migrations = "./migrations")]
async fn notices_name_comments_and_profiles_and_csam_needs_no_name(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (alice, bob) = (app.register("alice").await, app.register("bob").await);
    let post = app.upload(&alice, "A post").await;
    let comment = app.comment(&bob, post, "A comment").await;

    // A comment's own link.
    app.anon_post("/notices", notice(&format!("https://klarsocial.de/posts/{post}#comment-{comment}"))).await.ok();
    // A profile.
    app.anon_post("/notices", notice("https://www.klarsocial.eu/users/BOB")).await.ok();
    let targets = app.scalar("SELECT string_agg(target_type::text || ':' || target_id, ',' ORDER BY created_at) FROM reports").await.unwrap();
    assert_eq!(targets, format!("comment:{comment},user:{}", bob.id));

    // Child sexual abuse material can be reported anonymously (Art.
    // 16(2)(c) DSA); everything else needs a name and an address.
    let anonymous = json!({
        "reason": "csam", "explanation": "Shows a child.", "content_url": format!("/posts/{post}"),
        "notifier_name": null, "notifier_email": null, "good_faith": true
    });
    app.anon_post("/notices", anonymous).await.ok();
    assert_eq!(app.count("SELECT 1 FROM content_notices WHERE notifier_email IS NULL").await, 1);

    // Sent while signed in: part of that account's data export.
    let signed_in = app.post(&alice, "/notices", notice(&format!("/posts/{post}#comment-{comment}"))).await;
    assert_eq!(signed_in.status, StatusCode::CREATED);
    assert_eq!(app.export(&alice).await["moderation"]["notices_filed"].as_array().unwrap().len(), 1);
}
