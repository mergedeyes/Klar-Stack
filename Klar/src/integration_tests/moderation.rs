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
        (app.register("alice").await, app.trusted("bob").await, app.trusted("carol").await, app.admin().await);

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
    let (alice, bob, admin) = (app.register("alice").await, app.trusted("bob").await, app.admin().await);

    let post = app.upload(&alice, "Warned").await;
    let report = app.report(&bob, "post", post, "self_harm").await;
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
    let listed = queue["objections"].as_array().unwrap().iter().find(|o| o["id"] == id.as_str()).unwrap().clone();
    assert_eq!(listed["pending_reports"], 1);

    // While the report behind the warning is pending, deciding the report
    // answers the objection, so the two can't contradict each other.
    let resolve = format!("/admin/moderation/decisions/{id}/objection");
    assert_eq!(app.post(&admin, &resolve, json!({ "accept": true, "response": " " })).await.status, StatusCode::BAD_REQUEST);
    assert_eq!(
        app.post(&admin, &resolve, json!({ "accept": true, "response": "You're right." })).await.status,
        StatusCode::CONFLICT
    );
    let reports = app.get(&admin, "/admin/reports").await.ok().json();
    assert_eq!(reports[0]["objections"][0]["decision_id"], id.as_str(), "the objection shows on its report");
    app.post(&admin, &format!("/admin/reports/{report}/dismiss"), json!({ "objection_response": "You're right." })).await.ok();

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
    let (carol, bob, admin) = (app.register("carol").await, app.trusted("bob").await, app.admin().await);

    let post = app.upload(&carol, "csam").await;
    let report = app.report(&bob, "post", post, "csam").await;
    assert_eq!(app.scalar(&format!("SELECT moderation_status FROM posts WHERE id = '{post}'")).await.as_deref(), Some("hidden"));
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
    assert_eq!(queue[0]["reports"][0]["id"], old_report.to_string());
    assert_eq!(queue[0]["overdue"], true);
    assert_eq!(queue[1]["overdue"], false);

    let evidence = app.get(&admin, "/admin/evidence").await.ok().json();
    assert_eq!(evidence[0]["target_id"], old.to_string());
    assert_eq!(evidence[0]["overdue"], true);
}

#[sqlx::test(migrations = "./migrations")]
async fn statements_are_exported_and_outlive_the_account_without_excerpt(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (alice, bob) = (app.register("alice").await, app.trusted("bob").await);

    let post = app.upload(&alice, "Reported").await;
    app.report(&bob, "post", post, "violence").await;
    let id = decisions(&app, &alice).await[0]["id"].as_str().unwrap().to_string();

    let export = app.export(&alice).await;
    assert_eq!(export["moderation"]["moderation_decisions"].as_array().unwrap().len(), 1);
    assert!(export["notifications_received"].as_array().unwrap().iter().any(|n| n["from"] == "Klar"));
    assert_eq!(app.export(&bob).await["moderation"]["reports_filed"].as_array().unwrap().len(), 1);

    app.delete_account(&alice).await.ok();
    assert_eq!(
        app.scalar(&format!("SELECT affected_user_id IS NULL AND content_excerpt IS NULL FROM moderation_decisions WHERE id = '{id}'")).await.as_deref(),
        Some("true")
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn a_removal_replaces_an_objected_automatic_warning(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (alice, bob, admin) = (app.register("alice").await, app.trusted("bob").await, app.admin().await);

    let post = app.upload(&alice, "Warned").await;
    let report = app.report(&bob, "post", post, "violence").await;
    let warning = decisions(&app, &alice).await[0]["id"].as_str().unwrap().to_string();
    app.post(&alice, &format!("/moderation/decisions/{warning}/objection"), json!({ "text": "It's a film scene." })).await.ok();

    app.post(&admin, &format!("/admin/reports/{report}/remove"), json!({})).await.ok();
    let d = app.get(&alice, &format!("/moderation/decisions/{warning}")).await.ok().json();
    assert_eq!(d["objection_status"], "superseded");
    assert_eq!(d["can_object"], false);
    assert_eq!(notices(&app, &alice, "objection_resolved").await.len(), 1);
    assert!(app.get(&admin, "/admin/moderation").await.ok().json()["objections"].as_array().unwrap().is_empty());
    // The removal can be objected to itself.
    let removal = decisions(&app, &alice).await.into_iter().find(|d| d["restriction"] == "removed").unwrap();
    assert_eq!(removal["can_object"], true);
}

#[sqlx::test(migrations = "./migrations")]
async fn removed_content_is_restored_on_objection_or_deleted_after_the_window(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (alice, bob, carol, admin) =
        (app.register("alice").await, app.register("bob").await, app.register("carol").await, app.admin().await);
    let post_count = || async { app.scalar(&format!("SELECT post_count FROM users WHERE id = '{}'", alice.id)).await.unwrap() };
    let remove = |target_type: &'static str, id: uuid::Uuid| {
        let (app, bob, admin) = (&app, bob.clone(), admin.clone());
        async move {
            let report = app.report(&bob, target_type, id, "harassment").await;
            app.post(&admin, &format!("/admin/reports/{report}/remove"), json!({})).await.ok();
        }
    };

    // Removed: gone for everyone, its author included, and off the count.
    let post = app.upload(&alice, "Harassing post").await;
    remove("post", post).await;
    assert_eq!(app.get(&bob, &format!("/posts/{post}")).await.status, StatusCode::NOT_FOUND);
    assert_eq!(app.get(&alice, &format!("/posts/{post}")).await.status, StatusCode::NOT_FOUND);
    assert_eq!(post_count().await, "0");

    // An accepted objection brings it back as it was.
    let removal = decisions(&app, &alice).await[0]["id"].as_str().unwrap().to_string();
    app.post(&alice, &format!("/moderation/decisions/{removal}/objection"), json!({ "text": "It was a quote." })).await.ok();
    app.post(&admin, &format!("/admin/moderation/decisions/{removal}/objection"), json!({ "accept": true, "response": "Agreed." }))
        .await
        .ok();
    assert_eq!(app.get(&bob, &format!("/posts/{post}")).await.ok().json()["caption"], "Harassing post");
    assert_eq!(post_count().await, "1");

    // Without an objection it is deleted for good once the window is over.
    let other = app.upload(&alice, "Another one").await;
    let key_before = app.scalar(&format!("SELECT full_key FROM media_assets WHERE post_id = '{other}'")).await;
    remove("post", other).await;
    eventually("key rotation", || async {
        app.scalar(&format!("SELECT full_key FROM media_assets WHERE post_id = '{other}'")).await != key_before
    })
    .await;
    let key = app.scalar(&format!("SELECT full_key FROM media_assets WHERE post_id = '{other}'")).await.unwrap();
    crate::retention::sweep(&app.state).await;
    assert_eq!(app.count(&format!("SELECT 1 FROM posts WHERE id = '{other}'")).await, 1, "not yet");
    app.exec(&format!("UPDATE moderation_decisions SET created_at = NOW() - INTERVAL '184 days' WHERE target_id = '{other}'")).await;
    crate::retention::sweep(&app.state).await;
    assert_eq!(app.count(&format!("SELECT 1 FROM posts WHERE id = '{other}'")).await, 0);
    assert!(!app.media_file(&key).exists());
    assert_eq!(
        app.scalar(&format!("SELECT content_purged_at IS NOT NULL FROM moderation_decisions WHERE target_id = '{other}'")).await.as_deref(),
        Some("true")
    );

    // A removed comment that others replied to stays as an empty
    // placeholder, so the replies keep their place.
    let bobs = app.upload(&bob, "Bob's post").await;
    let comment = app.comment(&alice, bobs, "Insulting comment").await;
    let reply = app
        .post(&carol, &format!("/posts/{bobs}/comments"), json!({ "body": "Please don't", "parent_comment_id": comment }))
        .await
        .ok()
        .json();
    remove("comment", comment).await;
    let thread = app.get(&bob, &format!("/posts/{bobs}/comments")).await.ok().json();
    assert!(!thread.to_string().contains("Insulting"));
    let placeholder = thread.as_array().unwrap().iter().find(|c| c["id"] == comment.to_string()).unwrap().clone();
    assert_eq!(placeholder["body"], "");
    assert!(thread.to_string().contains("Please don't"));

    app.exec(&format!("UPDATE moderation_decisions SET created_at = NOW() - INTERVAL '184 days' WHERE target_id = '{comment}'")).await;
    crate::retention::sweep(&app.state).await;
    assert_eq!(app.scalar(&format!("SELECT body FROM comments WHERE id = '{comment}'")).await.as_deref(), Some(""));
    app.delete(&carol, &format!("/posts/{bobs}/comments/{}", reply["id"].as_str().unwrap())).await.ok();
    crate::retention::sweep(&app.state).await;
    assert_eq!(app.count(&format!("SELECT 1 FROM comments WHERE id = '{comment}'")).await, 0, "no replies left");
}
