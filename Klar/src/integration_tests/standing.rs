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
    // Removed for everyone, but kept until the objection window has passed.
    assert_eq!(app.scalar(&format!("SELECT moderation_status FROM comments WHERE id = '{comment}'")).await.as_deref(), Some("removed"));
    let visible = app.get(&bob, &format!("/posts/{post}/comments")).await.ok();
    assert!(!String::from_utf8_lossy(&visible.body).contains("nobody cares"), "the comment is gone");

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
    let (alice, bob, admin) = (app.register("alice").await, app.trusted("bob").await, app.admin().await);

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
    assert_eq!(app.patch(&alice, "/users/me", json!({ "username": "alice2" })).await.status, StatusCode::FORBIDDEN);
    // ... except emptying the profile, e.g. of an offending bio or picture.
    app.patch(&alice, "/users/me", json!({ "bio": "", "is_private": false })).await.ok();
    app.delete(&alice, "/users/me/avatar").await.ok();
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

    let comment = removed_comment(&app, &alice, &bob, &admin, Some("harassment_targeted")).await;
    let ban = app
        .post(&admin, "/admin/users/alice/measures", json!({ "measure": "ban", "reason": "harassment", "explanation": "Threatened several people." }))
        .await
        .ok()
        .json();
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
    assert_eq!(
        app.scalar(&format!("SELECT moderation_status FROM comments WHERE id = '{comment}'")).await.as_deref(),
        Some("visible"),
        "the removed comment is restored"
    );
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
        let measure = json!({ "measure": "ban", "reason": "harassment", "explanation": "Threatened several people." });
        app.post(&admin, &format!("/admin/users/{name}/measures"), measure).await.ok();
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

#[sqlx::test(migrations = "./migrations")]
async fn likely_illegal_classifications_are_preserved_and_flagged_for_the_authorities(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (alice, bob, admin) = (app.register("alice").await, app.register("bob").await, app.admin().await);

    // Reported as spam, found to be Holocaust denial: preserved anyway.
    let post = app.upload(&bob, "a post").await;
    let comment = app.comment(&alice, post, "Denial text").await;
    let report = app.report(&bob, "comment", comment, "spam").await;
    assert_eq!(app.count(&format!("SELECT 1 FROM evidence_records WHERE target_id = '{comment}'")).await, 0, "spam copies nothing");
    app.post(
        &admin,
        &format!("/admin/reports/{report}/remove"),
        json!({ "violation": "extremism_promotion", "justification": "Denies the Holocaust" }),
    )
    .await
    .ok();
    let evidence = app.get(&admin, "/admin/evidence").await.ok().json();
    let record = evidence.as_array().unwrap().iter().find(|e| e["target_id"] == comment.to_string()).unwrap().clone();
    assert!(record["reasons"].as_array().unwrap().iter().any(|r| r == "extremism"));
    assert_eq!(record["authority_report"], "recommended");
    assert_eq!(record["authority_reported"], false);
    assert_eq!(record["decision"], "removed");
    assert!(record["content_deleted_at"].is_null(), "kept, unseen, for a possible objection");
    assert_eq!(
        app.count(&format!("SELECT 1 FROM evidence_versions v WHERE v.evidence_id = '{}'", record["id"].as_str().unwrap())).await,
        1,
        "the content as it was removed"
    );

    // An attack threat: a required report, listed first and never purged
    // before it is recorded.
    let threat = app.upload(&alice, "Tomorrow at the school").await;
    let report = app.report(&bob, "post", threat, "terrorism").await;
    app.post(&admin, &format!("/admin/reports/{report}/remove"), json!({ "violation": "terror_threat" })).await.ok();
    let evidence = app.get(&admin, "/admin/evidence").await.ok().json();
    assert_eq!(evidence[0]["target_id"], threat.to_string(), "an unrecorded required report comes first");
    assert_eq!(evidence[0]["authority_report"], "required");
    let id = evidence[0]["id"].as_str().unwrap().to_string();

    app.exec("UPDATE evidence_records SET retain_until = NOW() - INTERVAL '1 day'").await;
    crate::evidence::sweep(&app.state).await;
    assert!(app.scalar(&format!("SELECT purged_at FROM evidence_records WHERE id = '{id}'")).await.is_none(), "kept");
    assert!(
        app.scalar(&format!("SELECT purged_at FROM evidence_records WHERE target_id = '{comment}'")).await.is_some(),
        "a recommended one follows the normal retention"
    );

    app.post(&admin, &format!("/admin/evidence/{id}/authority-report"), json!({ "authority": "LKA Berlin", "reported_on": "2026-09-30" }))
        .await
        .ok();
    crate::evidence::sweep(&app.state).await;
    assert!(app.scalar(&format!("SELECT purged_at FROM evidence_records WHERE id = '{id}'")).await.is_some());
}

#[sqlx::test(migrations = "./migrations")]
async fn measures_say_why_and_close_the_reports_behind_them(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (carol, bob, dave, admin) =
        (app.register("carol").await, app.register("bob").await, app.register("dave").await, app.admin().await);
    let report = app.report(&bob, "user", carol.id, "impersonation").await;
    let elsewhere = app.report(&bob, "user", dave.id, "spam").await;
    let path = "/admin/users/carol/measures";

    // Without strikes the score explains nothing, so the admin has to.
    assert_eq!(app.post(&admin, path, json!({ "measure": "warning", "reason": "impersonation" })).await.status, StatusCode::BAD_REQUEST);
    // Only reports on this account can be closed by its measure.
    let wrong = json!({ "measure": "warning", "reason": "impersonation", "explanation": "x", "report_ids": [elsewhere] });
    assert_eq!(app.post(&admin, path, wrong).await.status, StatusCode::CONFLICT);
    let measure = json!({
        "measure": "warning", "reason": "impersonation",
        "explanation": "Dein Profil gibt sich als eine bekannte Journalistin aus.",
        "report_ids": [report],
    });
    app.post(&admin, path, measure).await.ok();

    // The statement names the facts it relies on, and no score.
    let ds = app.get(&carol, "/moderation/decisions").await.ok().json();
    let explanation = ds[0]["explanation"].as_str().unwrap();
    assert!(explanation.contains("bekannte Journalistin"), "{explanation}");
    assert!(explanation.contains("Meldung deines Kontos"), "{explanation}");
    assert!(!explanation.contains("Punkten"), "{explanation}");
    assert_eq!(ds[0]["source"], "notice");

    // The report is closed as acted on, and the reporter learns that.
    let mine = app.get(&bob, "/moderation/reports").await.ok().json();
    let mine = mine.as_array().unwrap().iter().find(|r| r["id"] == report.to_string()).unwrap().clone();
    assert_eq!(mine["status"], "actioned");
    assert_eq!(mine["outcome"], "account_measure");
    let outcomes = app.get(&bob, "/notifications").await.ok().json();
    assert_eq!(outcomes.as_array().unwrap().iter().filter(|n| n["type_name"] == "report_outcome").count(), 1);
    assert_eq!(app.scalar(&format!("SELECT status FROM reports WHERE id = '{elsewhere}'")).await.as_deref(), Some("pending"));
}

#[sqlx::test(migrations = "./migrations")]
async fn account_measures_record_their_source_and_the_log_filters_by_it(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (carol, dave, admin) = (app.register("carol").await, app.register("dave").await, app.admin().await);

    // Decided from the standing page without a report: the team's own
    // initiative, and the statement says so.
    let own = json!({ "measure": "warning", "reason": "impersonation", "explanation": "Gibt sich als Klar aus." });
    app.post(&admin, "/admin/users/carol/measures", own).await.ok();
    let d = app.get(&carol, "/moderation/decisions").await.ok().json()[0].clone();
    assert_eq!(d["source"], "own_initiative");
    let explanation = d["explanation"].as_str().unwrap();
    assert!(explanation.contains("eigenen Prüfung auf dein Konto"), "{explanation}");
    assert!(!explanation.contains("Meldung deines Kontos"), "{explanation}");

    // Resting on an authority's order opened as a case on the account: the
    // order is the source, named with its reference.
    let order = json!({
        "target_type": "user", "target_id": dave.id, "reason": "terrorism", "source": "authority_order",
        "authority": "Bundeskriminalamt", "order_reference": "TCO-2026-18",
    });
    let report = app.post(&admin, "/admin/cases", order).await.ok().json()["report_id"].as_str().unwrap().to_string();
    let measure = json!({ "measure": "ban", "reason": "terrorism", "explanation": "Verbreitet Terrorpropaganda.", "report_ids": [report] });
    app.post(&admin, "/admin/users/dave/measures", measure).await.ok();
    let d = app.get(&dave, "/moderation/decisions").await.ok().json()[0].clone();
    assert_eq!(d["source"], "authority_order");
    let explanation = d["explanation"].as_str().unwrap();
    assert!(explanation.contains("Anordnung von Bundeskriminalamt (Aktenzeichen TCO-2026-18)"), "{explanation}");
    assert!(d["ground"].as_str().unwrap().starts_with("Behördliche Anordnung"), "{}", d["ground"]);

    // Every decision has a source, so each filter finds its own.
    let log = |source: &str| {
        let (app, admin, source) = (&app, admin.clone(), source.to_string());
        async move {
            let rows = app.get(&admin, &format!("/admin/decisions?source={source}")).await.ok().json();
            rows.as_array().unwrap().iter().map(|d| d["affected_username"].as_str().unwrap().to_string()).collect::<Vec<_>>()
        }
    };
    assert_eq!(log("own_initiative").await, ["carol"]);
    assert_eq!(log("authority_order").await, ["dave"]);
    assert!(log("notice").await.is_empty());
    assert_eq!(app.count("SELECT 1 FROM moderation_decisions WHERE source IS NULL").await, 0);
}

#[sqlx::test(migrations = "./migrations")]
async fn the_standing_page_lists_the_pending_reports_on_the_account(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (carol, bob, admin) = (app.register("carol").await, app.register("bob").await, app.admin().await);
    let on_account = app.report(&bob, "user", carol.id, "impersonation").await;
    // A report on carol's post isn't one a measure answers.
    app.report(&bob, "post", app.upload(&carol, "Sunset").await, "spam").await;

    let pending = app.get(&admin, "/admin/users/carol/standing").await.ok().json()["pending_reports"].clone();
    assert_eq!(pending.as_array().unwrap().len(), 1);
    assert_eq!(pending[0]["id"], on_account.to_string());
    assert_eq!(pending[0]["reason"], "impersonation");
    assert_eq!(pending[0]["source"], "user_report");

    let measure = json!({ "measure": "warning", "reason": "impersonation", "explanation": "Gibt sich als Klar aus.", "report_ids": [on_account] });
    let after = app.post(&admin, "/admin/users/carol/measures", measure).await.ok().json();
    assert!(after["pending_reports"].as_array().unwrap().is_empty(), "answered by the measure");
}
