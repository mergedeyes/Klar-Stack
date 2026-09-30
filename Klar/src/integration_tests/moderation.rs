//! Statements of reasons, objections and report outcomes (moderation.rs,
//! handlers/moderation.rs, handlers/reports.rs).

use axum::http::StatusCode;
use serde_json::{json, Value};
use sqlx::PgPool;

use super::support::*;

async fn notices(app: &TestApp, user: &User, kind: &str) -> Vec<Value> {
    let all = app.get(user, "/notifications").await.ok().json();
    all.as_array().unwrap().iter().filter(|n| n["type_name"] == kind).cloned().collect()
}

async fn decisions(app: &TestApp, user: &User) -> Vec<Value> {
    app.get(user, "/moderation/decisions").await.ok().json().as_array().unwrap().clone()
}

#[sqlx::test(migrations = "./migrations")]
async fn automatic_warning_gets_a_statement_and_dismissal_lifts_it(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (alice, bob, carol, admin) =
        (app.register("alice").await, app.register("bob").await, app.register("carol").await, app.admin().await);

    let post = app.upload(&alice, "A violent caption").await;
    let report = app.report(&bob, "post", post, "violence").await;

    let ds = decisions(&app, &alice).await;
    assert_eq!(ds.len(), 1);
    let d = &ds[0];
    assert_eq!(d["restriction"], "flagged");
    assert_eq!(d["automated"], true);
    assert_eq!(d["ground_type"], "terms");
    assert!(d["ground"].as_str().unwrap().contains("Abschnitt 4"));
    assert!(d["explanation"].as_str().unwrap().contains("Warnhinweis"));
    assert_eq!(d["content_excerpt"], "A violent caption");
    let id = d["id"].as_str().unwrap().to_string();

    let n = notices(&app, &alice, "moderation_decision").await;
    assert_eq!(n.len(), 1);
    assert!(n[0]["actor"].is_null(), "a notice from Klar, not a user");
    assert_eq!(n[0]["decision_id"], id.as_str());

    assert_eq!(app.get(&bob, &format!("/moderation/decisions/{id}")).await.status, StatusCode::NOT_FOUND);
    assert!(decisions(&app, &bob).await.is_empty());

    // A second warning-worthy report doesn't duplicate the statement.
    app.report(&carol, "post", post, "sexual_content").await;
    assert_eq!(decisions(&app, &alice).await.len(), 1);

    app.post(&admin, &format!("/admin/reports/{report}/dismiss"), json!({ "note": "internal note" })).await.ok();
    let d = app.get(&alice, &format!("/moderation/decisions/{id}")).await.ok().json();
    assert!(!d["lifted_at"].is_null());
    assert!(!d.to_string().contains("internal note"), "the review note stays internal");
    assert_eq!(app.scalar(&format!("SELECT moderation_status FROM posts WHERE id = '{post}'")).await.as_deref(), Some("visible"));
    assert_eq!(notices(&app, &alice, "moderation_decision").await.len(), 2, "author told of the lift");
    assert_eq!(notices(&app, &bob, "report_outcome").await.len(), 1, "reporter told the outcome");

    let mine = app.get(&bob, "/moderation/reports").await.ok().json();
    let mine = mine.as_array().unwrap().iter().find(|r| r["id"] == report.to_string()).unwrap();
    assert_eq!(mine["status"], "dismissed");
}

#[sqlx::test(migrations = "./migrations")]
async fn objections_are_answered_and_accepting_lifts_a_warning(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (alice, bob, admin) = (app.register("alice").await, app.register("bob").await, app.admin().await);

    let post = app.upload(&alice, "Warned").await;
    app.report(&bob, "post", post, "self_harm").await;
    let id = decisions(&app, &alice).await[0]["id"].as_str().unwrap().to_string();
    let path = format!("/moderation/decisions/{id}/objection");

    assert_eq!(app.post(&alice, &path, json!({ "text": "no" })).await.status, StatusCode::BAD_REQUEST);
    assert_eq!(app.post(&bob, &path, json!({ "text": "this is not mine at all" })).await.status, StatusCode::CONFLICT);
    let d = app.post(&alice, &path, json!({ "text": "It's an awareness campaign." })).await.ok().json();
    assert_eq!(d["objection_status"], "pending");
    assert_eq!(d["can_object"], false);
    assert_eq!(app.post(&alice, &path, json!({ "text": "and once more please" })).await.status, StatusCode::CONFLICT);

    assert_eq!(app.get(&alice, "/admin/moderation").await.status, StatusCode::FORBIDDEN);
    let queue = app.get(&admin, "/admin/moderation").await.ok().json();
    assert!(queue["objections"].as_array().unwrap().iter().any(|o| o["id"] == id.as_str()));

    let resolve = format!("/admin/moderation/decisions/{id}/objection");
    assert_eq!(app.post(&admin, &resolve, json!({ "accept": true, "response": " " })).await.status, StatusCode::BAD_REQUEST);
    app.post(&admin, &resolve, json!({ "accept": true, "response": "You're right." })).await.ok();

    let d = app.get(&alice, &format!("/moderation/decisions/{id}")).await.ok().json();
    assert_eq!(d["objection_status"], "accepted");
    assert_eq!(d["objection_response"], "You're right.");
    assert!(!d["lifted_at"].is_null());
    assert_eq!(app.scalar(&format!("SELECT moderation_status FROM posts WHERE id = '{post}'")).await.as_deref(), Some("visible"));
    assert_eq!(notices(&app, &alice, "objection_resolved").await.len(), 1);
}

#[sqlx::test(migrations = "./migrations")]
async fn csam_statements_are_held_back_until_released(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (carol, bob, admin) = (app.register("carol").await, app.register("bob").await, app.admin().await);

    let post = app.upload(&carol, "csam").await;
    let report = app.report(&bob, "post", post, "csam").await;
    assert!(decisions(&app, &carol).await.is_empty(), "the automatic hide isn't announced");
    assert!(notices(&app, &carol, "moderation_decision").await.is_empty());

    app.post(&admin, &format!("/admin/reports/{report}/remove"), json!({})).await.ok();
    let held: Vec<Value> = app.get(&admin, "/admin/moderation").await.ok().json()["held"]
        .as_array().unwrap().iter().filter(|d| d["target_id"] == post.to_string()).cloned().collect();
    assert_eq!(held.len(), 1, "only the removal; the superseded hide isn't listed");
    assert_eq!(held[0]["restriction"], "removed");
    assert!(decisions(&app, &carol).await.is_empty());
    assert_eq!(notices(&app, &bob, "report_outcome").await.len(), 1);

    let release = format!("/admin/moderation/decisions/{}/release", held[0]["id"].as_str().unwrap());
    assert_eq!(app.post(&admin, &release, json!({})).await.status, StatusCode::NO_CONTENT);
    let ds = decisions(&app, &carol).await;
    assert_eq!(ds.len(), 1);
    assert_eq!(ds[0]["ground_type"], "illegal");
    assert_eq!(ds[0]["automated"], false);
    assert_eq!(app.post(&admin, &release, json!({})).await.status, StatusCode::CONFLICT);
}

#[sqlx::test(migrations = "./migrations")]
async fn removals_get_statements_and_plain_reports_dont(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (alice, bob, admin) = (app.register("alice").await, app.register("bob").await, app.admin().await);

    let post = app.upload(&bob, "Bob's post").await;
    let comment = app.comment(&alice, post, "Harassing comment").await;
    let r = app.report(&bob, "comment", comment, "harassment").await;
    app.post(&admin, &format!("/admin/reports/{r}/remove"), json!({})).await.ok();

    let spam = app.upload(&alice, "Buy now").await;
    let r = app.report(&bob, "post", spam, "spam").await;
    assert_eq!(decisions(&app, &alice).await.len(), 1, "a spam report alone restricts nothing");
    app.post(&admin, &format!("/admin/reports/{r}/remove"), json!({})).await.ok();

    let ds = decisions(&app, &alice).await;
    assert!(ds.iter().any(|d| d["target_type"] == "comment" && d["restriction"] == "removed" && d["content_excerpt"] == "Harassing comment"));
    assert!(ds.iter().any(|d| d["content_excerpt"] == "Buy now" && d["restriction"] == "removed"));
}

#[sqlx::test(migrations = "./migrations")]
async fn overdue_reports_and_evidence_come_first(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (alice, bob, admin) = (app.register("alice").await, app.register("bob").await, app.admin().await);

    let fresh = app.upload(&alice, "Fresh").await;
    app.report(&bob, "post", fresh, "hate_speech").await;
    let old = app.upload(&alice, "Old").await;
    let old_report = app.report(&bob, "post", old, "hate_speech").await;
    app.exec(&format!("UPDATE reports SET created_at = NOW() - INTERVAL '31 days' WHERE id = '{old_report}'")).await;
    app.exec(&format!("UPDATE evidence_records SET created_at = NOW() - INTERVAL '31 days' WHERE target_id = '{old}'")).await;

    let queue = app.get(&admin, "/admin/reports").await.ok().json();
    assert_eq!(queue[0]["id"], old_report.to_string());
    assert_eq!(queue[0]["overdue"], true);
    assert_eq!(queue[1]["overdue"], false);

    let evidence = app.get(&admin, "/admin/evidence").await.ok().json();
    assert_eq!(evidence[0]["target_id"], old.to_string());
    assert_eq!(evidence[0]["overdue"], true);
}

#[sqlx::test(migrations = "./migrations")]
async fn statements_are_exported_and_outlive_the_account_without_excerpt(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (alice, bob) = (app.register("alice").await, app.register("bob").await);

    let post = app.upload(&alice, "Reported").await;
    app.report(&bob, "post", post, "violence").await;
    let id = decisions(&app, &alice).await[0]["id"].as_str().unwrap().to_string();

    let export = app.export(&alice).await;
    assert_eq!(export["moderation"]["moderation_decisions"].as_array().unwrap().len(), 1);
    assert!(export["notifications_received"].as_array().unwrap().iter().any(|n| n["from"] == "Klar"));
    assert_eq!(app.export(&bob).await["moderation"]["reports_filed"].as_array().unwrap().len(), 1);

    app.delete(&alice, "/users/me").await.ok();
    assert_eq!(
        app.scalar(&format!("SELECT affected_user_id IS NULL AND content_excerpt IS NULL FROM moderation_decisions WHERE id = '{id}'")).await.as_deref(),
        Some("true")
    );
}
