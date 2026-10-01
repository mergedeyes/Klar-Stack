//! Account reviews: signals, the candidate list, the logged review page and
//! its decisions (handlers/account_review.rs).

use axum::http::StatusCode;
use serde_json::{json, Value};
use sqlx::PgPool;

use super::support::*;

async fn open(app: &TestApp, admin: &User, username: &str) -> Value {
    app.post(admin, &format!("/admin/users/{username}/review"), json!({ "reason": "Spam report" })).await.ok().json()
}

#[sqlx::test(migrations = "./migrations")]
async fn a_spam_burst_flags_the_account_and_the_review_shows_its_activity(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (spammer, bob, admin) = (app.register("spammer").await, app.register("bob").await, app.admin().await);

    let post = app.upload(&bob, "Holiday").await;
    for _ in 0..12 {
        app.comment(&spammer, post, "Cheap coins at https://scam.example").await;
    }
    // Messaging needs a mutual follow.
    app.post(&spammer, "/users/bob/follow", json!({})).await.ok();
    app.post(&bob, "/users/spammer/follow", json!({})).await.ok();
    app.post(&spammer, "/chats/send", json!({ "receiver_id": bob.id, "body": "secret private words" })).await.ok();
    app.post(&spammer, &format!("/posts/{post}/like"), json!({})).await.ok();

    let candidates = app.get(&admin, "/admin/review-candidates").await.ok().json();
    let c = candidates.as_array().unwrap().iter().find(|c| c["username"] == "spammer").unwrap().clone();
    assert!(c["flags"].as_array().unwrap().iter().any(|f| f == "burst"));
    assert!(c["flags"].as_array().unwrap().iter().any(|f| f == "duplicates"));
    assert!(!candidates.as_array().unwrap().iter().any(|c| c["username"] == "bob"), "one post is not a signal");
    assert_eq!(app.get(&bob, "/admin/review-candidates").await.status, StatusCode::FORBIDDEN);

    // Opening a review needs a reason and is recorded.
    let path = "/admin/users/spammer/review";
    assert_eq!(app.post(&admin, path, json!({ "reason": " " })).await.status, StatusCode::BAD_REQUEST);
    assert_eq!(app.post(&bob, path, json!({ "reason": "curious" })).await.status, StatusCode::FORBIDDEN);
    let r = open(&app, &admin, "spammer").await;
    assert_eq!(app.scalar("SELECT reason FROM account_reviews").await.as_deref(), Some("Spam report"));
    assert_eq!(app.scalar("SELECT reviewer_id::text FROM account_reviews").await, Some(admin.id.to_string()));

    assert_eq!(r["overview"]["username"], "spammer");
    assert_eq!(r["comments"].as_array().unwrap().len(), 12);
    assert_eq!(r["comments"][0]["post_author"], "bob");
    assert_eq!(r["likes"][0]["post_author"], "bob");
    assert!(r["signals"]["links"].as_i64().unwrap() >= 12);
    // Direct messages as numbers, never their text.
    assert!(!r.to_string().contains("secret private words"));
    assert_eq!(r["messages"]["sent_7d"], 1);
    assert_eq!(r["follows"][0]["username"], "bob");
}

#[sqlx::test(migrations = "./migrations")]
async fn decisions_lock_ban_or_close_the_review(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (alice, carol, dave, admin) =
        (app.register("alice").await, app.register("carol").await, app.register("dave").await, app.admin().await);

    // No action.
    let r = open(&app, &admin, "dave").await;
    let decide = format!("/admin/reviews/{}/decide", r["review_id"].as_str().unwrap());
    assert_eq!(app.post(&admin, &decide, json!({ "outcome": "no_action", "note": "" })).await.status, StatusCode::BAD_REQUEST);
    assert_eq!(app.post(&admin, &decide, json!({ "outcome": "maybe", "note": "x" })).await.status, StatusCode::BAD_REQUEST);
    app.post(&admin, &decide, json!({ "outcome": "no_action", "note": "Normal activity" })).await.ok();
    assert_eq!(app.post(&admin, &decide, json!({ "outcome": "lock", "note": "x" })).await.status, StatusCode::CONFLICT);
    app.get(&dave, "/users/me").await.ok();

    // Hijacked: locked, the owner emailed.
    let r = open(&app, &admin, "alice").await;
    app.post(&admin, &format!("/admin/reviews/{}/decide", r["review_id"].as_str().unwrap()), json!({ "outcome": "lock", "note": "Sudden crypto spam" }))
        .await
        .ok();
    assert_eq!(app.get(&alice, "/users/me").await.status, StatusCode::UNAUTHORIZED);
    assert_eq!(app.scalar("SELECT note FROM account_locks").await.as_deref(), Some("Sudden crypto spam"));

    // A bot: permanently suspended, with a statement that says why.
    let r = open(&app, &admin, "carol").await;
    app.post(&admin, &format!("/admin/reviews/{}/decide", r["review_id"].as_str().unwrap()), json!({ "outcome": "bot", "note": "Posts every 30 s, same text" }))
        .await
        .ok();
    let s = app.get(&carol, "/users/me/standing").await.ok().json();
    assert_eq!(s["suspension"]["permanent"], true);
    let ds = app.get(&carol, "/moderation/decisions").await.ok().json();
    assert!(ds[0]["explanation"].as_str().unwrap().contains("automatisiert betrieben"));
    assert!(!ds.to_string().contains("every 30 s"), "the review note stays internal");

    let outcomes = app.scalar("SELECT string_agg(outcome, ',' ORDER BY opened_at) FROM account_reviews").await;
    assert_eq!(outcomes.as_deref(), Some("no_action,locked,bot"));
}

#[sqlx::test(migrations = "./migrations")]
async fn a_review_answers_the_account_report_it_was_opened_from(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (bob, carol, dave, eve, admin) = (
        app.register("bob").await,
        app.register("carol").await,
        app.register("dave").await,
        app.register("eve").await,
        app.admin().await,
    );
    let review = |username: &'static str, report: uuid::Uuid, outcome: &'static str| {
        let (app, admin) = (&app, admin.clone());
        async move {
            let r = app
                .post(&admin, &format!("/admin/users/{username}/review"), json!({ "reason": "From a report", "report_id": report }))
                .await
                .ok()
                .json();
            let decide = format!("/admin/reviews/{}/decide", r["review_id"].as_str().unwrap());
            app.post(&admin, &decide, json!({ "outcome": outcome, "note": "Checked" })).await.ok();
        }
    };
    let state = |report: uuid::Uuid| {
        let app = &app;
        async move { app.scalar(&format!("SELECT status || '/' || COALESCE(outcome, '-') FROM reports WHERE id = '{report}'")).await.unwrap() }
    };

    // A bot reported as an account: the ban answers the report.
    let r = app.report(&bob, "user", carol.id, "spam").await;
    review("carol", r, "bot").await;
    assert_eq!(state(r).await, "actioned/account_measure");
    assert_eq!(
        app.scalar(&format!("SELECT '{r}' = ANY(report_ids) FROM moderation_decisions WHERE restriction = 'banned'")).await.as_deref(),
        Some("true")
    );

    // Nothing found: the account report is dismissed.
    let r = app.report(&bob, "user", dave.id, "impersonation").await;
    review("dave", r, "no_action").await;
    assert_eq!(state(r).await, "dismissed/no_violation");

    // A hijacked account: the lock doesn't decide its spam post, which
    // stays in the queue.
    let post = app.upload(&eve, "Cheap coins").await;
    let r = app.report(&bob, "post", post, "spam").await;
    review("eve", r, "lock").await;
    assert_eq!(state(r).await, "pending/-");
}
