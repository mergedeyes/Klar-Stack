//! In-app feedback (handlers/feedback.rs).

use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use super::support::*;
use crate::handlers::feedback::delete_expired;

#[sqlx::test(migrations = "./migrations")]
async fn feedback_is_validated_and_context_is_optional(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let tester = app.register("tester").await;

    assert_eq!(app.anon_post("/feedback", json!({ "category": "bug", "message": "hello there" })).await.status, StatusCode::UNAUTHORIZED);
    for (body, why) in [
        (json!({ "category": "rant", "message": "hello there" }), "bad category"),
        (json!({ "category": "bug", "message": "hi" }), "too short"),
        (json!({ "category": "bug", "message": "x".repeat(4001) }), "too long"),
    ] {
        assert_eq!(app.post(&tester, "/feedback", body).await.status, StatusCode::BAD_REQUEST, "{why}");
    }

    app.post(&tester, "/feedback", json!({
        "category": "bug", "message": "Feed doesn't load",
        "page_path": "/reset-password?token=SECRET#x", "user_agent": "UA/1.0", "viewport": "390×844"
    })).await.ok();
    assert_eq!(app.scalar("SELECT page_path FROM feedback WHERE category = 'bug'").await.as_deref(), Some("/reset-password"));
    assert_eq!(app.count("SELECT 1 FROM feedback WHERE page_path || user_agent || message LIKE '%SECRET%'").await, 0);

    app.post(&tester, "/feedback", json!({ "category": "idea", "message": "Dark mode toggle please" })).await.ok();
    assert_eq!(
        app.scalar("SELECT page_path IS NULL AND user_agent IS NULL AND viewport IS NULL FROM feedback WHERE category = 'idea'").await.as_deref(),
        Some("true")
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn admins_triage_feedback(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (tester, admin) = (app.register("tester").await, app.admin().await);
    app.post(&tester, "/feedback", json!({ "category": "bug", "message": "Something broke" })).await.ok();

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
    app.post(&tester, "/feedback", json!({ "category": "other", "message": "First message" })).await.ok();
    app.exec(&format!(
        "INSERT INTO feedback (user_id, category, message) SELECT '{}', 'other', 'filler' FROM generate_series(1, 19)",
        tester.id
    )).await;
    assert_eq!(app.post(&tester, "/feedback", json!({ "category": "bug", "message": "One more please" })).await.status, StatusCode::BAD_REQUEST);

    assert_eq!(app.export(&tester).await["feedback_sent"].as_array().unwrap().len(), 20);

    app.delete(&tester, "/users/me").await.ok();
    assert_eq!(app.count("SELECT 1 FROM feedback WHERE message = 'First message' AND user_id IS NULL").await, 1, "text stays, link goes");

    app.exec("UPDATE feedback SET created_at = NOW() - INTERVAL '366 days' WHERE message = 'filler'").await;
    assert_eq!(delete_expired(&app.state.db).await.unwrap(), 19);
    assert_eq!(app.count("SELECT 1 FROM feedback").await, 1);
}
