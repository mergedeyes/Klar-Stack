//! Who can see what: email addresses, private accounts and content hidden by
//! moderation (the P0 fixes from the 2026-09-29 review, users.rs, follows.rs,
//! posts.rs::require_visible_post).

use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use super::support::*;

/// Every response that shows another account: none may carry its email.
#[sqlx::test(migrations = "./migrations")]
async fn other_users_never_see_an_email_address(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (alice, bob, carol) = (app.register("alice").await, app.register("bob").await, app.register("carol").await);
    let post = app.upload(&bob, "bob's post").await;
    app.post(&alice, "/users/bob/follow", json!({})).await.ok();
    app.post(&bob, "/users/alice/follow", json!({})).await.ok();
    app.post(&alice, &format!("/posts/{post}/like"), json!({})).await.ok();
    app.comment(&alice, post, "nice").await;
    app.post(&bob, "/users/carol/block", json!({})).await.ok();
    app.post(&bob, "/chats/send", json!({ "receiver_id": alice.id, "body": "hi alice" })).await.ok();

    // Each response must show the other account (so an empty list can't
    // pass), but never its address.
    for (viewer, path, shows) in [
        (&carol, "/users/alice", "alice"),
        (&carol, "/users/search?q=ali", "alice"),
        (&carol, "/users/bob/followers", "alice"),
        (&carol, "/users/bob/following", "alice"),
        (&carol, &format!("/posts/{post}") as &str, "bob"),
        // Only a count, no list of who liked.
        (&carol, &format!("/posts/{post}/likes"), "like_count"),
        (&carol, &format!("/posts/{post}/comments"), "alice"),
        (&carol, "/feed/discovery", "bob"),
        (&bob, "/notifications", "alice"),
        (&bob, "/users/me/blocked", "carol"),
        (&bob, "/chats", "alice"),
    ] {
        let res = app.get(viewer, path).await.ok();
        let body = String::from_utf8_lossy(&res.body);
        assert!(body.contains(&format!("\"{shows}\"")), "{path} doesn't show {shows}: {body}");
        assert!(!body.contains("@example.test"), "{path} leaks an email address: {body}");
    }
    // The owner still gets their own.
    let me = app.get(&alice, "/users/me").await.ok().json();
    assert_eq!(me["email"], "alice@example.test");
}

#[sqlx::test(migrations = "./migrations")]
async fn a_private_accounts_posts_need_an_accepted_follow(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (alice, bob) = (app.register("alice").await, app.register("bob").await);
    app.patch(&alice, "/users/me", json!({ "is_private": true })).await.ok();
    let post = app.upload(&alice, "private holiday").await;
    let own_comment = app.comment(&alice, post, "my own comment").await;

    let reads = [
        format!("/posts/{post}"),
        format!("/posts/{post}/media"),
        format!("/posts/{post}/comments"),
        format!("/posts/{post}/likes"),
        "/users/alice/posts".to_string(),
        "/users/alice/followers".to_string(),
        "/users/alice/following".to_string(),
    ];
    let assert_locked_out = |expected: StatusCode| {
        let app = &app;
        let bob = &bob;
        let reads = &reads;
        async move {
            for path in reads {
                assert_eq!(app.get(bob, path).await.status, expected, "GET {path}");
            }
            for (path, body) in [
                (format!("/posts/{post}/like"), json!({})),
                (format!("/posts/{post}/comments"), json!({ "body": "let me in" })),
                (format!("/posts/{post}/comments/{own_comment}/like"), json!({})),
            ] {
                assert_eq!(app.post(bob, &path, body).await.status, expected, "POST {path}");
            }
        }
    };

    // Not following: nothing hanging off the post is readable or writable,
    // and the profile itself stays visible.
    assert_locked_out(StatusCode::FORBIDDEN).await;
    app.get(&bob, "/users/alice").await.ok();
    assert!(app.get(&bob, "/feed/discovery").await.ok().json()["data"].as_array().unwrap().is_empty());

    // A pending request isn't enough.
    app.post(&bob, "/users/alice/follow", json!({})).await.ok();
    assert_locked_out(StatusCode::FORBIDDEN).await;

    // Accepted: everything opens up.
    app.post(&alice, "/users/me/follow-requests/bob/accept", json!({})).await.ok();
    for path in &reads {
        app.get(&bob, path).await.ok();
    }
    app.post(&bob, &format!("/posts/{post}/like"), json!({})).await.ok();
    app.comment(&bob, post, "lovely").await;

    // Unfollowing closes it again.
    app.delete(&bob, "/users/alice/follow").await.ok();
    assert_eq!(app.get(&bob, &format!("/posts/{post}")).await.status, StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = "./migrations")]
async fn hidden_content_is_visible_only_to_its_owner(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (alice, bob, carol) = (app.register("alice").await, app.register("bob").await, app.register("carol").await);
    let post = app.upload(&alice, "reported post").await;
    let other_post = app.upload(&carol, "carol's post").await;
    let comment = app.comment(&alice, other_post, "reported comment").await;
    app.report(&bob, "post", post, "csam").await;
    app.report(&bob, "comment", comment, "ncii").await;

    // To everyone else the hidden post doesn't exist (404, like a missing
    // post), including everything hanging off it.
    for path in [format!("/posts/{post}"), format!("/posts/{post}/media"), format!("/posts/{post}/comments")] {
        assert_eq!(app.get(&carol, &path).await.status, StatusCode::NOT_FOUND, "GET {path}");
    }
    assert_eq!(app.post(&carol, &format!("/posts/{post}/like"), json!({})).await.status, StatusCode::NOT_FOUND);
    assert_eq!(
        app.post(&carol, &format!("/posts/{post}/comments"), json!({ "body": "hm" })).await.status,
        StatusCode::NOT_FOUND
    );
    let discovery = app.get(&carol, "/feed/discovery").await.ok().json();
    let ids: Vec<_> = discovery["data"].as_array().unwrap().iter().map(|p| p["id"].as_str().unwrap().to_string()).collect();
    assert!(!ids.contains(&post.to_string()), "hidden post in discovery");
    let comments = app.get(&carol, &format!("/posts/{other_post}/comments")).await.ok();
    assert!(!String::from_utf8_lossy(&comments.body).contains("reported comment"));

    // The owner still sees both, marked as hidden.
    let own = app.get(&alice, &format!("/posts/{post}")).await.ok().json();
    assert_eq!(own["moderation_status"], "hidden");
    app.get(&alice, &format!("/posts/{post}/media")).await.ok();
    let own_comments = app.get(&alice, &format!("/posts/{other_post}/comments")).await.ok();
    assert!(String::from_utf8_lossy(&own_comments.body).contains("reported comment"));
}
