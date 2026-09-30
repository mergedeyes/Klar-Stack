//! Account standing: strikes from removals, suggested measures, suspensions
//! and what a suspended account can still do (standing.rs,
//! handlers/standing.rs).

use axum::http::StatusCode;
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use super::support::*;

/// Reports a comment by `author` for `reason` and has the admin remove it,
/// classified as `violation` (None: the reason's default type).
async fn removed_for(app: &TestApp, author: &User, reporter: &User, admin: &User, reason: &str, violation: Option<&str>) -> Uuid {
    let post = app.upload(reporter, "a post").await;
    let comment = app.comment(author, post, "you suck").await;
    let report = app.report(reporter, "comment", comment, reason).await;
    let mut body = json!({});
    if let Some(v) = violation {
        body["violation"] = json!(v);
        body["justification"] = json!("test");
    }
    app.post(admin, &format!("/admin/reports/{report}/remove"), body).await.ok();
    comment
}

/// A removed harassment report: "insult" (5), "harassment_targeted" (20)
/// or "threat_stalking" (40).
async fn removed_comment(app: &TestApp, author: &User, reporter: &User, admin: &User, violation: Option<&str>) -> Uuid {
    removed_for(app, author, reporter, admin, "harassment", violation).await
}

async fn standing(app: &TestApp, admin: &User, username: &str) -> Value {
    app.get(admin, &format!("/admin/users/{username}/standing")).await.ok().json()
}

#[sqlx::test(migrations = "./migrations")]
async fn a_few_minor_insults_stay_below_a_warning(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (alice, bob, admin) = (app.register("alice").await, app.register("bob").await, app.admin().await);

    for _ in 0..3 {
        removed_comment(&app, &alice, &bob, &admin, None).await;
    }

    let mine = app.get(&alice, "/users/me/standing").await.ok().json();
    assert_eq!(mine["score"], 18, "5 + 5, and the third within 30 days counts 1.5 times: 8");
    assert_eq!(mine["strikes"].as_array().unwrap().len(), 3);
    assert_eq!(mine["strikes"][0]["severity"], "minor");
    assert_eq!(mine["strikes"][0]["violation"], "insult");
    assert_eq!(mine["strikes"][0]["violation_label_de"], "Einzelne Beleidigung");
    assert_eq!(mine["strikes"][0]["base_points"], 5);
    assert_eq!(mine["strikes"][0]["points"], 8);
    assert_eq!(mine["strikes"][1]["points"], 5);
    assert!(!mine["strikes"][0]["expires_at"].is_null(), "minor strikes expire");
    assert!(mine["suspension"].is_null());

    let s = standing(&app, &admin, "alice").await;
    assert!(s["suggestion"].is_null());
    let listed = app.get(&admin, "/admin/standing").await.ok().json();
    assert!(listed.as_array().unwrap().is_empty(), "below the warning threshold");

    assert_eq!(app.get(&alice, "/admin/standing").await.status, StatusCode::FORBIDDEN);
    assert_eq!(app.get(&alice, "/admin/users/alice/standing").await.status, StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = "./migrations")]
async fn the_catalog_sets_points_and_departing_from_the_report_needs_a_justification(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (alice, bob, admin) = (app.register("alice").await, app.register("bob").await, app.admin().await);

    let catalog = app.get(&admin, "/admin/violations").await.ok().json();
    assert!(catalog.as_array().unwrap().iter().any(|v| v["id"] == "threat_stalking" && v["severity"] == "serious"));
    assert_eq!(app.get(&alice, "/admin/violations").await.status, StatusCode::FORBIDDEN);

    removed_comment(&app, &alice, &bob, &admin, Some("none")).await;
    assert_eq!(standing(&app, &admin, "alice").await["score"], 0);

    removed_comment(&app, &alice, &bob, &admin, Some("threat_stalking")).await;
    let s = standing(&app, &admin, "alice").await;
    assert_eq!(s["score"], 40);
    assert_eq!(s["suggestion"], "warning");

    let post = app.upload(&bob, "x").await;
    let comment = app.comment(&alice, post, "x").await;
    let report = app.report(&bob, "comment", comment, "harassment").await;
    let remove = format!("/admin/reports/{report}/remove");
    assert_eq!(app.post(&admin, &remove, json!({ "violation": "huge" })).await.status, StatusCode::BAD_REQUEST);
    assert_eq!(
        app.post(&admin, &remove, json!({ "violation": "none" })).await.status,
        StatusCode::BAD_REQUEST,
        "no strike needs a justification"
    );
    assert_eq!(
        app.post(&admin, &remove, json!({ "violation": "terror_propaganda", "justification": " " })).await.status,
        StatusCode::BAD_REQUEST,
        "another reason than the report's needs a justification"
    );
    app.post(&admin, &remove, json!({ "violation": "terror_propaganda", "justification": "Shares IS videos" })).await.ok();
    let s = standing(&app, &admin, "alice").await;
    assert_eq!(s["score"], 100, "40 + 100, capped");
    assert_eq!(s["suggestion"], "ban", "terrorist propaganda suggests a permanent suspension at once");

    // The statement names the type, its criterion and the points, and
    // cites the ground of what the team found; the justification stays
    // internal.
    let ds = app.get(&alice, "/moderation/decisions").await.ok().json();
    let d = &ds[0];
    assert_eq!(d["reason"], "terrorism");
    let explanation = d["explanation"].as_str().unwrap();
    assert!(explanation.contains("Eingestuft als „Terroristische Propaganda“"), "{explanation}");
    assert!(explanation.contains("100 Punkte"), "{explanation}");
    assert!(!d.to_string().contains("IS videos"));
    assert_eq!(
        app.scalar(&format!("SELECT violation_note FROM moderation_decisions WHERE id = '{}'", d["id"].as_str().unwrap())).await.as_deref(),
        Some("Shares IS videos")
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn strikes_keep_the_removed_content_with_its_context(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (alice, bob, admin) = (app.register("alice").await, app.register("bob").await, app.admin().await);

    let post = app.upload(&bob, "My holiday photo").await;
    let parent = app.comment(&bob, post, "Had a great time").await;
    let res = app
        .post(&alice, &format!("/posts/{post}/comments"), json!({ "body": "nobody cares, loser", "parent_comment_id": parent }))
        .await
        .ok()
        .json();
    let comment = res["id"].as_str().unwrap();
    let report = app.report(&bob, "comment", comment.parse().unwrap(), "harassment").await;
    app.post(&admin, &format!("/admin/reports/{report}/remove"), json!({ "violation": "harassment_targeted" })).await.ok();
    assert_eq!(app.count(&format!("SELECT 1 FROM comments WHERE id = '{comment}'")).await, 0, "the comment is gone");

    let strike = standing(&app, &admin, "alice").await["strikes"][0].clone();
    let open = format!("/admin/strikes/{}/open", strike["id"].as_str().unwrap());
    assert_eq!(app.post(&alice, &open, json!({})).await.status, StatusCode::FORBIDDEN);
    let d = app.post(&admin, &open, json!({})).await.ok().json();
    assert_eq!(d["snapshot"]["content"]["text"], "nobody cares, loser");
    assert_eq!(d["snapshot"]["context"]["post"]["text"], "My holiday photo");
    assert_eq!(d["snapshot"]["context"]["parent_comment"]["text"], "Had a great time");
    assert_eq!(d["snapshot"]["context"]["parent_comment"]["author"], "bob");
    assert_eq!(d["snapshot"]["reports"][0]["reason"], "harassment");
    assert!(d["snapshot"]["reports"][0].get("reporter_id").is_none(), "not who reported");
    assert!(!d["decided_at"].is_null());
    assert!(d["criterion_de"].as_str().unwrap().contains("wiederholt oder gezielt"));
    assert_eq!(app.count(&format!("SELECT 1 FROM strike_views WHERE admin_id = '{}'", admin.id)).await, 1, "opening is logged");

    // The author's export has their own text, not the context.
    let export = app.export(&alice).await;
    let exported = &export["moderation"]["account_strikes"][0];
    assert_eq!(exported["removed_content"]["text"], "nobody cares, loser");
    assert!(!exported.to_string().contains("holiday"));

    // The snapshot goes with the strike.
    app.exec(&format!("UPDATE account_strikes SET expires_at = NOW() - INTERVAL '1 second' WHERE user_id = '{}'", alice.id)).await;
    assert_eq!(app.post(&admin, &open, json!({})).await.status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "./migrations")]
async fn repeat_factor_counts_only_the_same_reason(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (alice, bob, admin) = (app.register("alice").await, app.register("bob").await, app.admin().await);

    removed_for(&app, &alice, &bob, &admin, "spam", None).await;
    removed_for(&app, &alice, &bob, &admin, "harassment", None).await;
    removed_for(&app, &alice, &bob, &admin, "spam", None).await;
    assert_eq!(standing(&app, &admin, "alice").await["score"], 15, "two spam and one harassment: no factor");

    removed_for(&app, &alice, &bob, &admin, "spam", None).await;
    assert_eq!(standing(&app, &admin, "alice").await["score"], 23, "third spam in 30 days: 8");

    // Outside the window, earlier strikes no longer count as repeats.
    app.exec(&format!("UPDATE account_strikes SET created_at = NOW() - INTERVAL '31 days' WHERE user_id = '{}'", alice.id)).await;
    removed_for(&app, &alice, &bob, &admin, "spam", None).await;
    let s = standing(&app, &admin, "alice").await;
    assert_eq!(s["strikes"][0]["points"], 5);
}

#[sqlx::test(migrations = "./migrations")]
async fn new_reasons_are_moderated_like_their_peers(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (alice, bob, admin) = (app.register("alice").await, app.register("bob").await, app.admin().await);

    // Non-consensual intimate images are hidden at once, like CSAM, but the
    // statement isn't held back.
    let post = app.upload(&alice, "private photo").await;
    let report = app.report(&bob, "post", post, "ncii").await;
    assert_eq!(app.scalar(&format!("SELECT moderation_status FROM posts WHERE id = '{post}'")).await.as_deref(), Some("hidden"));
    let ds = app.get(&alice, "/moderation/decisions").await.ok().json();
    assert_eq!(ds[0]["restriction"], "hidden");
    assert!(ds[0]["ground"].as_str().unwrap().contains("ohne Einwilligung"));
    assert_eq!(app.count(&format!("SELECT 1 FROM evidence_records WHERE target_id = '{post}'")).await, 1, "preserved");

    app.post(&admin, &format!("/admin/reports/{report}/remove"), json!({})).await.ok();
    let s = standing(&app, &admin, "alice").await;
    assert_eq!(s["strikes"][0]["severity"], "serious");

    // Terrorism gets a warning interstitial; fraud and illegal goods none.
    let t = app.upload(&alice, "threat").await;
    app.report(&bob, "post", t, "terrorism").await;
    assert_eq!(app.scalar(&format!("SELECT moderation_status FROM posts WHERE id = '{t}'")).await.as_deref(), Some("flagged"));
    for reason in ["fraud", "illegal_goods"] {
        let p = app.upload(&alice, reason).await;
        app.report(&bob, "post", p, reason).await;
        assert_eq!(app.scalar(&format!("SELECT moderation_status FROM posts WHERE id = '{p}'")).await.as_deref(), Some("visible"));
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn warning_comes_before_suspension_and_suspension_makes_the_account_read_only(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (alice, bob, admin) = (app.register("alice").await, app.register("bob").await, app.admin().await);
    let own_post = app.upload(&alice, "alice's post").await;
    let bob_post = app.upload(&bob, "bob's post").await;
    app.comment(&alice, bob_post, "a normal comment").await;

    removed_comment(&app, &alice, &bob, &admin, Some("threat_stalking")).await;
    removed_comment(&app, &alice, &bob, &admin, Some("harassment_targeted")).await;
    let s = standing(&app, &admin, "alice").await;
    assert_eq!(s["score"], 60);
    assert_eq!(s["suggestion"], "warning", "no suspension without a prior warning");

    let s = app.post(&admin, "/admin/users/alice/measures", json!({ "measure": "warning", "reason": "harassment" })).await.ok().json();
    assert!(s["suggestion"].is_null(), "nothing new since the warning");
    assert!(s["suspension"].is_null());

    removed_comment(&app, &alice, &bob, &admin, Some("insult")).await;
    let s = standing(&app, &admin, "alice").await;
    assert_eq!(s["score"], 68, "the third harassment strike counts 1.5 times");
    assert_eq!(s["suggestion"], "suspend_7d");

    let bad = app.post(&admin, "/admin/users/alice/measures", json!({ "measure": "forever", "reason": "harassment" })).await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);
    let s = app.post(&admin, "/admin/users/alice/measures", json!({ "measure": "suspend_7d", "reason": "harassment" })).await.ok().json();
    assert_eq!(s["suspension"]["permanent"], false);
    assert_eq!(s["measures"][0]["restriction"], "suspended");
    assert_eq!(s["measures"][0]["suspension_days"], 7);
    assert_eq!(s["measures"][0]["standing_score"], 68);

    // The statement reaches alice.
    let ds = app.get(&alice, "/moderation/decisions").await.ok().json();
    let suspension = ds.as_array().unwrap().iter().find(|d| d["restriction"] == "suspended").unwrap().clone();
    assert_eq!(suspension["target_type"], "user");
    assert!(suspension["explanation"].as_str().unwrap().contains("68 von 100"));

    // Read-only: writes are refused ...
    let refused = app.post(&alice, &format!("/posts/{bob_post}/comments"), json!({ "body": "hi" })).await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN);
    assert!(refused.json()["error"].as_str().unwrap().contains("suspended until"));
    assert_eq!(app.post(&alice, &format!("/posts/{bob_post}/like"), json!({})).await.status, StatusCode::FORBIDDEN);
    assert_eq!(app.post(&alice, "/users/bob/follow", json!({})).await.status, StatusCode::FORBIDDEN);
    assert_eq!(app.patch(&alice, "/users/me", json!({ "bio": "new" })).await.status, StatusCode::FORBIDDEN);
    // ... reading, objecting, exporting and deleting own content still work.
    app.get(&alice, "/feed").await.ok();
    app.get(&alice, "/users/alice").await.ok();
    assert_eq!(app.export(&alice).await["moderation"]["account_strikes"].as_array().unwrap().len(), 3);
    let objection = format!("/moderation/decisions/{}/objection", suspension["id"].as_str().unwrap());
    app.post(&alice, &objection, json!({ "text": "I've changed, honestly." })).await.ok();
    app.delete(&alice, &format!("/posts/{own_post}")).await.ok();

    // Hidden from everyone else.
    assert_eq!(app.get(&bob, "/users/alice").await.status, StatusCode::NOT_FOUND);
    assert_eq!(app.anon_get("/users/alice/posts").await.status, StatusCode::NOT_FOUND);
    assert_eq!(app.anon_get("/users/alice/stats").await.status, StatusCode::NOT_FOUND);
    assert!(app.get(&bob, "/users/search?q=alice").await.ok().json().as_array().unwrap().is_empty());
    let comments = app.get(&bob, &format!("/posts/{bob_post}/comments")).await.ok().json();
    assert!(comments.as_array().unwrap().iter().all(|c| c["username"] != "alice"));

    let listed = app.get(&admin, "/admin/standing").await.ok().json();
    assert_eq!(listed[0]["username"], "alice");

    // Lifted early: everything is back.
    app.post(&admin, "/admin/users/alice/lift-suspension", json!({})).await.ok();
    assert_eq!(app.post(&admin, "/admin/users/alice/lift-suspension", json!({})).await.status, StatusCode::CONFLICT);
    app.get(&bob, "/users/alice").await.ok();
    app.post(&alice, &format!("/posts/{bob_post}/comments"), json!({ "body": "hi again" })).await.ok();
    let d = app.get(&alice, &format!("/moderation/decisions/{}", suspension["id"].as_str().unwrap())).await.ok().json();
    assert!(!d["lifted_at"].is_null());
    assert_eq!(
        app.scalar(&format!("SELECT lifted_by FROM moderation_decisions WHERE id = '{}'", suspension["id"].as_str().unwrap())).await,
        Some(admin.id.to_string()),
        "who lifted it is recorded"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn accepted_objections_end_a_suspension_and_take_back_a_strike(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (alice, bob, admin) = (app.register("alice").await, app.register("bob").await, app.admin().await);

    removed_comment(&app, &alice, &bob, &admin, Some("harassment_targeted")).await;
    let ban = app.post(&admin, "/admin/users/alice/measures", json!({ "measure": "ban", "reason": "harassment" })).await.ok().json();
    assert_eq!(ban["suspension"]["permanent"], true);
    assert!(ban["suspension"]["until"].is_null());
    let refused = app.post(&alice, "/posts", json!({ "caption": "x" })).await;
    assert!(refused.json()["error"].as_str().unwrap().contains("permanently"));

    let ds = app.get(&alice, "/moderation/decisions").await.ok().json();
    let ds = ds.as_array().unwrap();
    let ban_id = ds.iter().find(|d| d["restriction"] == "banned").unwrap()["id"].as_str().unwrap().to_string();
    let removal_id = ds.iter().find(|d| d["restriction"] == "removed").unwrap()["id"].as_str().unwrap().to_string();

    for id in [&ban_id, &removal_id] {
        app.post(&alice, &format!("/moderation/decisions/{id}/objection"), json!({ "text": "This was a misunderstanding." })).await.ok();
        app.post(&admin, &format!("/admin/moderation/decisions/{id}/objection"), json!({ "accept": true, "response": "Agreed." })).await.ok();
    }

    let mine = app.get(&alice, "/users/me/standing").await.ok().json();
    assert!(mine["suspension"].is_null(), "suspension ended");
    assert_eq!(mine["score"], 0, "strike taken back");
    app.get(&bob, "/users/alice").await.ok();
}

#[sqlx::test(migrations = "./migrations")]
async fn held_back_strikes_are_hidden_from_the_user(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (carol, bob, admin) = (app.register("carol").await, app.register("bob").await, app.admin().await);

    let post = app.upload(&carol, "csam").await;
    let report = app.report(&bob, "post", post, "csam").await;
    app.post(&admin, &format!("/admin/reports/{report}/remove"), json!({})).await.ok();

    let s = standing(&app, &admin, "carol").await;
    assert_eq!(s["score"], 100);
    assert_eq!(s["suggestion"], "ban");
    let mine = app.get(&carol, "/users/me/standing").await.ok().json();
    assert_eq!(mine["score"], 0, "not revealed before the statement is released");
    assert!(mine["strikes"].as_array().unwrap().is_empty());
}

#[sqlx::test(migrations = "./migrations")]
async fn a_permanently_suspended_account_is_deleted_after_the_objection_window(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (alice, carol, bob, admin) =
        (app.register("alice").await, app.register("carol").await, app.register("bob").await, app.admin().await);

    removed_comment(&app, &alice, &bob, &admin, Some("harassment_targeted")).await;
    removed_comment(&app, &carol, &bob, &admin, Some("harassment_targeted")).await;
    for name in ["alice", "carol"] {
        app.post(&admin, &format!("/admin/users/{name}/measures"), json!({ "measure": "ban", "reason": "harassment" })).await.ok();
    }
    let mine = app.get(&alice, "/users/me/standing").await.ok().json();
    assert!(!mine["suspension"]["deletion_at"].is_null(), "the user sees when");
    let ban = |name: &str| format!(
        "SELECT d.id FROM moderation_decisions d JOIN users u ON u.suspension_decision_id = d.id WHERE u.username = '{name}'"
    );
    let (alice_ban, carol_ban) = (app.scalar(&ban("alice")).await.unwrap(), app.scalar(&ban("carol")).await.unwrap());

    // Carol objects: her deletion waits for the outcome.
    app.post(&carol, &format!("/moderation/decisions/{carol_ban}/objection"), json!({ "text": "Please look at this again." })).await.ok();
    assert!(app.get(&carol, "/users/me/standing").await.ok().json()["suspension"]["deletion_at"].is_null());

    // Not yet due: nothing happens.
    crate::standing::sweep_bans(&app.state).await;
    assert!(app.scalar(&format!("SELECT deletion_notified_at FROM moderation_decisions WHERE id = '{alice_ban}'")).await.is_none());

    // Two weeks before: the reminder, but no deletion yet.
    app.exec("UPDATE moderation_decisions SET created_at = NOW() - INTERVAL '170 days' WHERE restriction = 'banned'").await;
    crate::standing::sweep_bans(&app.state).await;
    assert!(app.scalar(&format!("SELECT deletion_notified_at FROM moderation_decisions WHERE id = '{alice_ban}'")).await.is_some());
    assert!(app.scalar(&format!("SELECT deletion_notified_at FROM moderation_decisions WHERE id = '{carol_ban}'")).await.is_none());
    app.get(&alice, "/users/me").await.ok();

    // Past the window, but the reminder is only a day old: still waits.
    app.exec("UPDATE moderation_decisions SET created_at = NOW() - INTERVAL '190 days' WHERE restriction = 'banned'").await;
    crate::standing::sweep_bans(&app.state).await;
    app.get(&alice, "/users/me").await.ok();

    // Reminder two weeks old: the account goes, the decision stays.
    app.exec("UPDATE moderation_decisions SET deletion_notified_at = NOW() - INTERVAL '15 days' WHERE deletion_notified_at IS NOT NULL").await;
    crate::standing::sweep_bans(&app.state).await;
    assert_eq!(app.count("SELECT 1 FROM users WHERE username = 'alice'").await, 0);
    assert!(app.scalar(&format!("SELECT account_deleted_at FROM moderation_decisions WHERE id = '{alice_ban}'")).await.is_some());
    assert!(app.scalar(&format!("SELECT affected_user_id FROM moderation_decisions WHERE id = '{alice_ban}'")).await.is_none());
    assert_eq!(app.count("SELECT 1 FROM users WHERE username = 'carol'").await, 1, "objection pending");
}
