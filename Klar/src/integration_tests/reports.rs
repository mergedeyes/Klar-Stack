//! The report workflow (handlers/reports.rs, moderation.rs): one queue card
//! per item, decisions that act on all of its reports, restrictions that
//! only their own reports can lift, decisions on content its author already
//! deleted, limits for reporters, reported direct messages and admin alerts.

use axum::http::StatusCode;
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use super::support::*;

async fn status(app: &TestApp, post: Uuid) -> String {
    app.scalar(&format!("SELECT moderation_status FROM posts WHERE id = '{post}'")).await.unwrap()
}

async fn report_state(app: &TestApp, report: Uuid) -> String {
    app.scalar(&format!("SELECT status || '/' || COALESCE(outcome, '-') FROM reports WHERE id = '{report}'")).await.unwrap()
}

async fn notices(app: &TestApp, user: &User, kind: &str) -> usize {
    let all = app.get(user, "/notifications").await.ok().json();
    all.as_array().unwrap().iter().filter(|n| n["type_name"] == kind).count()
}

#[sqlx::test(migrations = "./migrations")]
async fn dismissing_a_report_keeps_restrictions_that_rest_on_something_else(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (alice, bob, carol, admin) =
        (app.register("alice").await, app.trusted("bob").await, app.register("carol").await, app.admin().await);

    // Hidden after a CSAM report; someone else reports it as spam, and that
    // report alone is dismissed.
    let post = app.upload(&alice, "x").await;
    let csam = app.report(&bob, "post", post, "csam").await;
    let spam = app.report(&carol, "post", post, "spam").await;
    let queue = app.get(&admin, "/admin/reports").await.ok().json();
    assert_eq!(queue.as_array().unwrap().len(), 1, "one card per item");
    assert_eq!(queue[0]["reports"].as_array().unwrap().len(), 2);
    assert_eq!(queue[0]["severity"], "critical");
    assert!(queue[0]["target_thumb_url"].is_null(), "no CSAM thumbnails in a list");

    app.post(&admin, &format!("/admin/reports/{spam}/dismiss"), json!({ "only_this": true })).await.ok();
    assert_eq!(status(&app, post).await, "hidden");
    assert_eq!(report_state(&app, csam).await, "pending/-");
    assert_eq!(
        app.count(&format!("SELECT 1 FROM moderation_decisions WHERE target_id = '{post}' AND lifted_at IS NOT NULL")).await,
        0
    );

    // Hidden after an accepted rights claim: a dismissed report doesn't undo
    // that either.
    let photo = app.upload(&alice, "Sunset").await;
    let claim = app
        .anon_post(
            "/rights-claims",
            json!({
                "claim_type": "copyright", "claimant_name": "Erika", "claimant_email": "erika@example.test",
                "claimant_organization": null, "represented_party": null,
                "content_url": format!("https://www.klarsocial.eu/posts/{photo}"),
                "work_description": "My photo", "ownership_basis": "I took it",
                "original_url": null, "good_faith": true, "website": ""
            }),
        )
        .await
        .ok()
        .json();
    app.post(&admin, &format!("/admin/rights-claims/{}/accept", claim["id"].as_str().unwrap()), json!({})).await.ok();
    let report = app.report(&carol, "post", photo, "spam").await;
    app.post(&admin, &format!("/admin/reports/{report}/dismiss"), json!({})).await.ok();
    assert_eq!(status(&app, photo).await, "hidden");
    assert_eq!(
        app.scalar(&format!("SELECT status FROM rights_claims WHERE id = '{}'", claim["id"].as_str().unwrap())).await.as_deref(),
        Some("accepted")
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn a_removal_decides_every_report_on_the_item(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (alice, bob, carol, admin) =
        (app.register("alice").await, app.register("bob").await, app.register("carol").await, app.admin().await);

    let post = app.upload(&alice, "Hateful").await;
    let r1 = app.report(&bob, "post", post, "harassment").await;
    let r2 = app.report(&carol, "post", post, "hate_speech").await;
    app.post(&admin, &format!("/admin/reports/{r2}/remove"), json!({})).await.ok();

    for r in [r1, r2] {
        assert_eq!(report_state(&app, r).await, "actioned/removed");
    }
    assert_eq!(
        app.scalar(&format!(
            "SELECT cardinality(report_ids) FROM moderation_decisions WHERE target_id = '{post}' AND restriction = 'removed'"
        ))
        .await
        .as_deref(),
        Some("2"),
        "one decision for both"
    );
    assert_eq!(app.count(&format!("SELECT 1 FROM account_strikes WHERE user_id = '{}'", alice.id)).await, 1);
    for reporter in [&bob, &carol] {
        assert_eq!(notices(&app, reporter, "report_outcome").await, 1);
    }
    let mine = app.get(&carol, "/moderation/reports").await.ok().json();
    assert_eq!(mine[0]["outcome"], "removed");
    assert!(app.get(&admin, "/admin/reports").await.ok().json().as_array().unwrap().is_empty());
}

#[sqlx::test(migrations = "./migrations")]
async fn deleting_reported_content_does_not_escape_the_decision(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (carol, bob, dave, admin) =
        (app.register("carol").await, app.trusted("bob").await, app.register("dave").await, app.admin().await);

    // Reported as CSAM, deleted by its author before anyone looked.
    let post = app.upload(&carol, "csam").await;
    let report = app.report(&bob, "post", post, "csam").await;
    app.delete(&carol, &format!("/posts/{post}")).await.ok();

    let group = app.get(&admin, "/admin/reports").await.ok().json()[0].clone();
    assert_eq!(group["target_exists"], false);
    assert_eq!(group["target_username"], "carol", "from the evidence copy");
    assert!(!group["evidence_id"].is_null());

    // Confirming the violation decides on the preserved copy: a removal
    // with its strike, so the account comes up for a ban.
    app.post(&admin, &format!("/admin/reports/{report}/remove"), json!({})).await.ok();
    let explanation = app
        .scalar(&format!("SELECT explanation FROM moderation_decisions WHERE target_id = '{post}' AND restriction = 'removed'"))
        .await
        .unwrap();
    assert!(explanation.contains("bereits selbst gelöscht"), "{explanation}");
    let standing = app.get(&admin, "/admin/users/carol/standing").await.ok().json();
    assert_eq!(standing["score"], 100);
    assert_eq!(standing["suggestion"], "ban");
    assert_eq!(standing["strikes"][0]["content_excerpt"], "csam");
    assert_eq!(report_state(&app, report).await, "actioned/removed");

    // A spam report on content deleted before review has nothing left to
    // decide: it is closed, and the reporter told the content is gone.
    let spam = app.upload(&dave, "Buy now").await;
    let spam_report = app.report(&bob, "post", spam, "spam").await;
    app.delete(&dave, &format!("/posts/{spam}")).await.ok();
    assert_eq!(report_state(&app, spam_report).await, "obsolete/obsolete");
    let mine = app.get(&bob, "/moderation/reports").await.ok().json();
    let mine = mine.as_array().unwrap().iter().find(|r| r["id"] == spam_report.to_string()).unwrap().clone();
    assert_eq!(mine["outcome"], "obsolete");
}

#[sqlx::test(migrations = "./migrations")]
async fn reporters_are_limited_and_only_trusted_ones_hide_content(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (alice, fresh, trusted, admin) =
        (app.register("alice").await, app.register("fresh").await, app.trusted("trusted").await, app.admin().await);
    let report = |user: &User, post: Uuid, reason: &str| {
        let body = json!({ "target_type": "post", "target_id": post, "reason": reason });
        let (app, user) = (&app, user.clone());
        async move { app.post(&user, "/reports", body).await }
    };

    // Not your own content, and not twice while the first one is pending.
    let post = app.upload(&alice, "post").await;
    assert_eq!(report(&alice, post, "spam").await.status, StatusCode::BAD_REQUEST);
    report(&trusted, post, "spam").await.ok();
    assert_eq!(report(&trusted, post, "spam").await.status, StatusCode::CONFLICT);

    // Content the reporter can't see doesn't exist for them.
    app.patch(&alice, "/users/me", json!({ "is_private": true })).await.ok();
    let private = app.upload(&alice, "private").await;
    assert_eq!(report(&fresh, private, "spam").await.status, StatusCode::NOT_FOUND);
    app.patch(&alice, "/users/me", json!({ "is_private": false })).await.ok();

    // A brand-new account's CSAM report queues at the top, without hiding.
    let reported = app.upload(&alice, "reported").await;
    report(&fresh, reported, "csam").await.ok();
    assert_eq!(status(&app, reported).await, "visible");
    let queue = app.get(&admin, "/admin/reports").await.ok().json();
    assert_eq!(queue[0]["target_id"], reported.to_string());
    assert_eq!(queue[0]["severity"], "critical");

    // A trusted account whose CSAM reports keep being dismissed loses that.
    for _ in 0..3 {
        app.exec(&format!(
            "INSERT INTO reports (reporter_id, target_type, target_id, reason, status, outcome, reviewed_at)
             VALUES ('{}', 'post', gen_random_uuid(), 'csam', 'dismissed', 'no_violation', NOW())",
            trusted.id
        ))
        .await;
    }
    let other = app.upload(&alice, "other").await;
    report(&trusted, other, "csam").await.ok();
    assert_eq!(status(&app, other).await, "visible");

    // At most five reports a day for the reasons that hide content.
    for i in 0..4 {
        let p = app.upload(&alice, &format!("p{i}")).await;
        report(&fresh, p, "ncii").await.ok();
    }
    let sixth = app.upload(&alice, "p5").await;
    assert_eq!(report(&fresh, sixth, "csam").await.status, StatusCode::TOO_MANY_REQUESTS);
    report(&fresh, sixth, "spam").await.ok();
}

#[sqlx::test(migrations = "./migrations")]
async fn reported_messages_are_preserved_with_context_and_outlive_their_sender(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (alice, bob, carol, admin) =
        (app.register("alice").await, app.register("bob").await, app.register("carol").await, app.admin().await);
    app.post(&alice, "/users/bob/follow", json!({})).await.ok();
    app.post(&bob, "/users/alice/follow", json!({})).await.ok();
    let send = |from: &User, to: &User, body: String| {
        let (app, from, to) = (&app, from.clone(), to.clone());
        async move { app.post(&from, "/chats/send", json!({ "receiver_id": to.id, "body": body })).await.ok().json() }
    };
    for i in 0..12 {
        let (from, to) = if i % 2 == 0 { (&alice, &bob) } else { (&bob, &alice) };
        send(from, to, format!("message {i}")).await;
    }
    let spam: Value = send(&alice, &bob, "Buy cheap coins".into()).await;
    let threat: Value = send(&alice, &bob, "I know where you live".into()).await;
    let conversation = threat["conversation_id"].as_str().unwrap().to_string();
    let message = |m: &Value| m["id"].as_str().unwrap().parse::<Uuid>().unwrap();

    // Only the recipient can report, and not their own message.
    let body = |id: Uuid| json!({ "target_type": "message", "target_id": id, "reason": "harassment" });
    assert_eq!(app.post(&carol, "/reports", body(message(&threat))).await.status, StatusCode::NOT_FOUND);
    assert_eq!(app.post(&alice, "/reports", body(message(&threat))).await.status, StatusCode::BAD_REQUEST);
    let threat_report = app.report(&bob, "message", message(&threat), "harassment").await;
    let spam_report = app.report(&bob, "message", message(&spam), "spam").await;

    // Every message report is preserved, with the ten messages before it.
    let context = app
        .scalar(&format!(
            "SELECT jsonb_array_length(v.content->'context') FROM evidence_versions v JOIN evidence_records e ON e.id = v.evidence_id
             WHERE e.target_id = '{}'",
            message(&threat)
        ))
        .await;
    assert_eq!(context.as_deref(), Some("10"));
    assert!(app.scalar(&format!("SELECT id FROM evidence_records WHERE target_id = '{}'", message(&spam))).await.is_some());

    // The queue never shows a message's text; its evidence record does.
    let queue = app.get(&admin, "/admin/reports").await.ok().json();
    assert!(!queue.to_string().contains("I know where you live"));

    // Removing a message deletes it for both, with a strike for its sender.
    app.post(&admin, &format!("/admin/reports/{spam_report}/remove"), json!({})).await.ok();
    let messages = app.get(&bob, &format!("/chats/{conversation}/messages")).await.ok().json();
    assert!(!messages.to_string().contains("Buy cheap coins"));
    assert_eq!(app.count(&format!("SELECT 1 FROM account_strikes WHERE user_id = '{}'", alice.id)).await, 1);

    // The sender deletes their account: the reported message survives as
    // evidence, and the report can still be decided.
    app.delete_account(&alice).await.ok();
    assert_eq!(
        app.scalar(&format!("SELECT deletion_trigger FROM evidence_records WHERE target_id = '{}'", message(&threat))).await.as_deref(),
        Some("account_deletion")
    );
    assert_eq!(report_state(&app, threat_report).await, "pending/-");
    app.post(&admin, &format!("/admin/reports/{threat_report}/remove"), json!({})).await.ok();
    assert_eq!(report_state(&app, threat_report).await, "actioned/removed");
    assert_eq!(notices(&app, &bob, "report_outcome").await, 2);
}

#[sqlx::test(migrations = "./migrations")]
async fn urgent_reports_alert_the_admins_once(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (alice, bob, carol, _admin) =
        (app.register("alice").await, app.register("bob").await, app.register("carol").await, app.admin().await);

    let post = app.upload(&alice, "x").await;
    app.report(&bob, "post", post, "terrorism").await;
    eventually("alert claimed", || async { app.count("SELECT 1 FROM admin_alerts WHERE kind = 'urgent_report'").await == 1 }).await;
    // The same item for the same reason doesn't alert again.
    app.report(&carol, "post", post, "terrorism").await;
    app.report(&carol, "post", app.upload(&alice, "y").await, "spam").await;
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    assert_eq!(app.count("SELECT 1 FROM admin_alerts").await, 1);

    let waiting = crate::alerts::attention(&app.state.db).await.unwrap();
    assert_eq!((waiting.reports, waiting.urgent_reports, waiting.overdue_reports), (2, 1, 0));
}

#[sqlx::test(migrations = "./migrations")]
async fn a_reporter_can_have_a_dismissal_checked_once_more(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (alice, bob, carol, admin) =
        (app.register("alice").await, app.register("bob").await, app.register("carol").await, app.admin().await);
    let post = app.upload(&alice, "Borderline").await;
    let report = app.report(&bob, "post", post, "hate_speech").await;
    app.post(&admin, &format!("/admin/reports/{report}/dismiss"), json!({})).await.ok();
    let mine = |user: &User| {
        let (app, user) = (&app, user.clone());
        async move { app.get(&user, "/moderation/reports").await.ok().json()[0].clone() }
    };
    assert_eq!(mine(&bob).await["can_recheck"], true);

    let recheck = format!("/moderation/reports/{report}/recheck");
    assert_eq!(app.post(&carol, &recheck, json!({})).await.status, StatusCode::CONFLICT, "not someone else's");
    let reopened = app.post(&bob, &recheck, json!({ "note": "Read the second sentence again." })).await.ok().json();
    assert_eq!(reopened["status"], "pending");
    assert_eq!(reopened["can_recheck"], false);
    assert_eq!(app.post(&bob, &recheck, json!({})).await.status, StatusCode::CONFLICT, "only once");

    // Back in the queue, marked as a re-check, and preserved again.
    let group = app.get(&admin, "/admin/reports").await.ok().json()[0].clone();
    assert_eq!(group["reports"][0]["recheck_note"], "Read the second sentence again.");
    assert!(!group["evidence_id"].is_null());
    app.post(&admin, &format!("/admin/reports/{report}/remove"), json!({})).await.ok();
    assert_eq!(mine(&bob).await["outcome"], "removed");
    assert_eq!(mine(&bob).await["can_recheck"], false);
}

#[sqlx::test(migrations = "./migrations")]
async fn the_team_acts_without_a_report_and_the_statement_says_so(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (alice, bob, admin) = (app.register("alice").await, app.register("bob").await, app.admin().await);
    let post = app.upload(&alice, "Join the attack").await;
    let case = |source: &str, extra: Value| {
        let mut body = json!({ "target_type": "post", "target_id": post, "reason": "terrorism", "source": source });
        for (k, v) in extra.as_object().unwrap() {
            body[k] = v.clone();
        }
        body
    };
    assert_eq!(app.post(&bob, "/admin/cases", case("own_initiative", json!({}))).await.status, StatusCode::FORBIDDEN);
    assert_eq!(app.post(&admin, "/admin/cases", case("authority_order", json!({}))).await.status, StatusCode::BAD_REQUEST);

    // An authority's order: at the top of the queue, preserved, and named
    // in the statement with its reference.
    let order = case("authority_order", json!({ "authority": "Bundeskriminalamt", "order_reference": "TCO-2026-17" }));
    let report = app.post(&admin, "/admin/cases", order).await.ok().json()["report_id"].as_str().unwrap().to_string();
    let group = app.get(&admin, "/admin/reports").await.ok().json()[0].clone();
    assert_eq!(group["severity"], "critical");
    assert_eq!(group["reports"][0]["source"], "authority_order");
    assert!(!group["evidence_id"].is_null());
    app.post(&admin, &format!("/admin/reports/{report}/remove"), json!({ "violation": "terror_propaganda" })).await.ok();
    let d = app.get(&alice, "/moderation/decisions").await.ok().json()[0].clone();
    assert_eq!(d["source"], "authority_order");
    assert!(d["ground"].as_str().unwrap().contains("Behördliche Anordnung (Bundeskriminalamt"), "{d}");
    assert!(d["explanation"].as_str().unwrap().contains("Aktenzeichen TCO-2026-17"), "{d}");
    assert!(app.get(&admin, "/moderation/reports").await.ok().json().as_array().unwrap().is_empty(), "not the admin's own report list");

    // The team's own finding.
    let other = app.upload(&alice, "Buy followers").await;
    let own = json!({ "target_type": "post", "target_id": other, "reason": "spam", "source": "own_initiative" });
    let report = app.post(&admin, "/admin/cases", own).await.ok().json()["report_id"].as_str().unwrap().to_string();
    app.post(&admin, &format!("/admin/reports/{report}/remove"), json!({})).await.ok();
    let d = app.get(&alice, "/moderation/decisions").await.ok().json()[0].clone();
    assert_eq!(d["source"], "own_initiative");
    assert!(d["explanation"].as_str().unwrap().contains("nicht auf einer Meldung"), "{d}");
}

#[sqlx::test(migrations = "./migrations")]
async fn admins_see_what_waits_and_every_decision(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (alice, bob, admin) = (app.register("alice").await, app.trusted("bob").await, app.admin().await);
    app.report(&bob, "post", app.upload(&alice, "violent").await, "violence").await;
    let spam = app.report(&bob, "post", app.upload(&alice, "spam").await, "spam").await;
    app.post(&admin, &format!("/admin/reports/{spam}/remove"), json!({})).await.ok();

    assert_eq!(app.get(&bob, "/admin/attention").await.status, StatusCode::FORBIDDEN);
    let waiting = app.get(&admin, "/admin/attention").await.ok().json();
    assert_eq!(waiting["reports"], 1);
    assert_eq!(waiting["total"], 1);

    let log = |query: &str| {
        let (app, admin, query) = (&app, admin.clone(), query.to_string());
        async move { app.get(&admin, &format!("/admin/decisions{query}")).await.ok().json().as_array().unwrap().clone() }
    };
    assert_eq!(app.get(&bob, "/admin/decisions").await.status, StatusCode::FORBIDDEN);
    let all = log("").await;
    assert_eq!(all.len(), 2);
    assert_eq!(all[0]["restriction"], "removed", "newest first");
    assert_eq!(all[0]["decided_by_username"], "site_admin");
    assert_eq!(all[0]["report_count"], 1);
    assert_eq!(log("?automated=true").await[0]["restriction"], "flagged");
    assert_eq!(log("?restriction=removed&decided_by=SITE_ADMIN").await.len(), 1);
    assert_eq!(log("?affected=alice").await.len(), 2);
    assert!(log("?affected=bob").await.is_empty());

    // Paging by the last row seen.
    let first = log("?limit=1").await;
    let cursor = format!(
        "?limit=1&before_time={}&before_id={}",
        urlencode(first[0]["created_at"].as_str().unwrap()),
        first[0]["id"].as_str().unwrap()
    );
    let second = log(&cursor).await;
    assert_eq!(second.len(), 1);
    assert_ne!(second[0]["id"], first[0]["id"]);
}

fn urlencode(text: &str) -> String {
    text.replace(':', "%3A").replace('+', "%2B")
}

#[sqlx::test(migrations = "./migrations")]
async fn closed_reports_list_what_the_queue_decided(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (alice, bob, admin) = (app.register("alice").await, app.trusted("bob").await, app.admin().await);
    let removed = app.report(&bob, "post", app.upload(&alice, "spam one").await, "spam").await;
    let dismissed = app.report(&bob, "post", app.upload(&alice, "fine").await, "spam").await;
    app.report(&bob, "post", app.upload(&alice, "still waiting").await, "spam").await;
    app.post(&admin, &format!("/admin/reports/{removed}/remove"), json!({})).await.ok();
    app.post(&admin, &format!("/admin/reports/{dismissed}/dismiss"), json!({ "note": "Satire, no spam" })).await.ok();

    assert_eq!(app.get(&bob, "/admin/reports/closed").await.status, StatusCode::FORBIDDEN);
    let closed = app.get(&admin, "/admin/reports/closed").await.ok().json();
    let closed = closed.as_array().unwrap();
    assert_eq!(closed.len(), 2, "the pending one isn't listed");
    // Most recently closed first; a dismissal leaves no decision.
    assert_eq!(closed[0]["id"], dismissed.to_string());
    assert_eq!(closed[0]["status"], "dismissed");
    assert_eq!(closed[0]["outcome"], "no_violation");
    assert_eq!(closed[0]["review_note"], "Satire, no spam");
    assert!(closed[0]["decision_id"].is_null());
    assert_eq!(closed[0]["reviewed_by_username"], "site_admin");
    assert_eq!(closed[0]["reporter_username"], "bob");
    assert_eq!(closed[0]["target_username"], "alice");
    assert_eq!(closed[1]["id"], removed.to_string());
    assert_eq!(closed[1]["outcome"], "removed");
    assert!(closed[1]["decision_id"].is_string());
    assert!(closed[1].get("target_preview").is_none(), "no content");

    let only_removed = app.get(&admin, "/admin/reports/closed?outcome=removed").await.ok().json();
    assert_eq!(only_removed.as_array().unwrap().len(), 1);
    let cursor = format!(
        "/admin/reports/closed?limit=1&before_time={}&before_id={}",
        urlencode(closed[0]["reviewed_at"].as_str().unwrap()),
        closed[0]["id"].as_str().unwrap()
    );
    let next = app.get(&admin, &cursor).await.ok().json();
    assert_eq!(next[0]["id"], removed.to_string());
}
