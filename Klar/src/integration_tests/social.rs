//! Follows, the home feed, blocks and the denormalized counters (follows.rs,
//! blocks.rs, likes.rs, comments.rs). The feed is fan-out-on-write, so every
//! follow, unfollow and block has to fill or clean feed_items, and every
//! write that changes a count has to change it in the same transaction.

use axum::http::StatusCode;
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use super::support::*;

async fn feed_ids(app: &TestApp, user: &User) -> Vec<String> {
    let feed = app.get(user, "/feed").await.ok().json();
    feed.as_array().unwrap().iter().map(|p| p["id"].as_str().unwrap().to_string()).collect()
}

async fn stats(app: &TestApp, user: &User, username: &str) -> (i64, i64, i64) {
    let s = app.get(user, &format!("/users/{username}/stats")).await.ok().json();
    (s["followers"].as_i64().unwrap(), s["following"].as_i64().unwrap(), s["posts"].as_i64().unwrap())
}

#[sqlx::test(migrations = "./migrations")]
async fn following_fills_the_feed_and_unfollowing_cleans_it(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (alice, bob) = (app.register("alice").await, app.register("bob").await);
    let older = app.upload(&alice, "before the follow").await.to_string();

    // Following backfills alice's existing posts; new ones fan out.
    app.post(&bob, "/users/alice/follow", json!({})).await.ok();
    assert_eq!(feed_ids(&app, &bob).await, vec![older.clone()]);
    let newer = app.upload(&alice, "after the follow").await.to_string();
    assert_eq!(feed_ids(&app, &bob).await, vec![newer.clone(), older.clone()]);
    assert_eq!(stats(&app, &bob, "alice").await, (1, 0, 2));
    assert_eq!(stats(&app, &bob, "bob").await, (0, 1, 0));

    // Following twice changes nothing.
    app.post(&bob, "/users/alice/follow", json!({})).await;
    assert_eq!(stats(&app, &bob, "alice").await, (1, 0, 2));

    app.delete(&bob, "/users/alice/follow").await.ok();
    assert!(feed_ids(&app, &bob).await.is_empty());
    assert_eq!(stats(&app, &bob, "alice").await, (0, 0, 2));
    assert_eq!(stats(&app, &bob, "bob").await, (0, 0, 0));

    // Deleting a post takes it out of followers' feeds and the count.
    app.post(&bob, "/users/alice/follow", json!({})).await.ok();
    app.delete(&alice, &format!("/posts/{newer}")).await.ok();
    assert_eq!(feed_ids(&app, &bob).await, vec![older]);
    assert_eq!(stats(&app, &bob, "alice").await, (1, 0, 1));
}

#[sqlx::test(migrations = "./migrations")]
async fn blocking_ends_follows_both_ways_and_stops_interaction(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (alice, bob) = (app.register("alice").await, app.register("bob").await);
    let post = app.upload(&alice, "alice's post").await;
    app.upload(&bob, "bob's post").await;
    app.post(&bob, "/users/alice/follow", json!({})).await.ok();
    app.post(&alice, "/users/bob/follow", json!({})).await.ok();
    // The home feed shows followed accounts only, not one's own posts.
    assert_eq!(feed_ids(&app, &bob).await.len(), 1);
    assert_eq!(feed_ids(&app, &alice).await.len(), 1);

    app.post(&alice, "/users/bob/block", json!({})).await.ok();
    assert_eq!(stats(&app, &alice, "alice").await, (0, 0, 1));
    assert_eq!(stats(&app, &alice, "bob").await, (0, 0, 1));
    assert!(feed_ids(&app, &bob).await.is_empty());
    assert!(feed_ids(&app, &alice).await.is_empty());

    // Neither side can follow, like or comment the other.
    for (user, path, body) in [
        (&bob, "/users/alice/follow".to_string(), json!({})),
        (&alice, "/users/bob/follow".to_string(), json!({})),
        (&bob, format!("/posts/{post}/like"), json!({})),
        (&bob, format!("/posts/{post}/comments"), json!({ "body": "hey" })),
    ] {
        // Refused with 400 ("Cannot follow this user" and the like).
        assert_eq!(app.post(user, &path, body).await.status, StatusCode::BAD_REQUEST, "POST {path}");
    }
    // And bob can't message alice any more (messaging needs mutual follows).
    assert_eq!(
        app.post(&bob, "/chats/send", json!({ "receiver_id": alice.id, "body": "hi" })).await.status,
        StatusCode::FORBIDDEN
    );

    // Unblocking doesn't restore the follows.
    app.delete(&alice, "/users/bob/block").await.ok();
    assert_eq!(stats(&app, &alice, "alice").await, (0, 0, 1));
    app.post(&bob, "/users/alice/follow", json!({})).await.ok();
}

#[sqlx::test(migrations = "./migrations")]
async fn counters_survive_double_clicks_and_comment_threads(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (alice, bob) = (app.register("alice").await, app.register("bob").await);
    let post = app.upload(&alice, "post").await;

    // Several likes racing (a double-click on a slow network): no error,
    // and the count matches the rows.
    let like_path = format!("/posts/{post}/like");
    let like = || app.post(&bob, &like_path, json!({}));
    let results = futures::future::join_all((0..6).map(|_| like())).await;
    assert!(results.iter().all(|r| r.status.is_success()), "{:?}", results.iter().map(|r| r.status).collect::<Vec<_>>());
    let counted = post_counts(&app, post).await;
    let rows = app.count(&format!("SELECT 1 FROM likes WHERE post_id = '{post}'")).await;
    assert_eq!(counted.0, rows);

    // A comment with two replies: deleting it takes all three off the count.
    let parent = app.comment(&bob, post, "parent").await;
    for body in ["reply one", "reply two"] {
        app.post(&alice, &format!("/posts/{post}/comments"), json!({ "body": body, "parent_comment_id": parent }))
            .await
            .ok();
    }
    app.comment(&alice, post, "unrelated").await;
    assert_eq!(post_counts(&app, post).await.1, 4);
    app.delete(&bob, &format!("/posts/{post}/comments/{parent}")).await.ok();
    assert_eq!(post_counts(&app, post).await.1, 1);
    assert_eq!(app.count(&format!("SELECT 1 FROM comments WHERE post_id = '{post}'")).await, 1);
}

async fn post_counts(app: &TestApp, post: Uuid) -> (i64, i64) {
    let row = sqlx::query_as::<_, (i64, i64)>("SELECT like_count, comment_count FROM posts WHERE id = $1")
        .bind(post)
        .fetch_one(&app.state.db)
        .await
        .unwrap();
    (row.0, row.1)
}

#[sqlx::test(migrations = "./migrations")]
async fn odd_query_parameters_are_harmless(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let alice = app.register("alice").await;
    app.register("al_ice").await;
    app.upload(&alice, "post").await;

    // Negative or huge limits are clamped instead of failing.
    for path in ["/feed?limit=-5", "/feed?limit=100000", "/users/alice/posts?limit=-1", "/feed/discovery?limit=0"] {
        app.get(&alice, path).await.ok();
    }

    // "%" and "_" in a search match literally, not as wildcards.
    let names = |res: Value| -> Vec<String> {
        res.as_array().unwrap().iter().map(|u| u["username"].as_str().unwrap().to_string()).collect()
    };
    assert!(names(app.anon_get("/users/search?q=%25").await.ok().json()).is_empty());
    assert_eq!(names(app.anon_get("/users/search?q=al_").await.ok().json()), vec!["al_ice"]);
}

/// Pages through `first` (the path with `?limit=`) until a short page,
/// following the cursor the way the frontend does.
async fn page_through(app: &TestApp, user: &User, first: &str, discovery: bool) -> Vec<String> {
    let mut ids = Vec::new();
    let mut path = first.to_string();
    for _ in 0..20 {
        let res = app.get(user, &path).await.ok().json();
        let page = if discovery { res["data"].clone() } else { res };
        let page = page.as_array().unwrap();
        ids.extend(page.iter().map(|p| p["id"].as_str().unwrap().to_string()));
        if page.len() < 7 {
            return ids;
        }
        let last = page.last().unwrap();
        let (time, id) = (last["created_at"].as_str().unwrap(), last["id"].as_str().unwrap());
        let cursor = if discovery { "cursor_time" } else { "cursor" };
        path = format!("{first}&{cursor}={}&cursor_id={id}", time.replace('+', "%2B"));
    }
    panic!("{first} never ended");
}

#[sqlx::test(migrations = "./migrations")]
async fn paging_never_skips_or_repeats_posts_from_the_same_second(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (alice, bob) = (app.register("alice").await, app.register("bob").await);
    app.post(&bob, "/users/alice/follow", json!({})).await.ok();
    let mut all = Vec::new();
    for i in 0..17 {
        all.push(app.upload(&alice, &format!("post {i}")).await.to_string());
    }
    // All in the same instant: only the id tie-breaker keeps the pages apart.
    app.exec("UPDATE posts SET created_at = '2026-09-01T12:00:00Z'").await;
    app.exec("UPDATE feed_items SET created_at = '2026-09-01T12:00:00Z'").await;
    all.sort();

    for (path, discovery) in [
        ("/users/alice/posts?limit=7", false),
        ("/feed?limit=7", false),
        ("/feed/discovery?limit=7", true),
    ] {
        let mut ids = page_through(&app, &bob, path, discovery).await;
        assert_eq!(ids.len(), 17, "{path}: {ids:?}");
        ids.sort();
        assert_eq!(ids, all, "{path} skipped or repeated a post");
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn deleting_an_account_lowers_the_counters_it_was_part_of(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (alice, bob, carol) = (app.register("alice").await, app.register("bob").await, app.register("carol").await);
    let post = app.upload(&alice, "alice's post").await;

    // Bob likes and comments; carol replies to his comment; bob likes
    // carol's own comment; bob follows alice, carol follows bob.
    app.post(&bob, &format!("/posts/{post}/like"), json!({})).await.ok();
    let bobs = app.comment(&bob, post, "nice").await;
    app.post(&carol, &format!("/posts/{post}/comments"), json!({ "body": "agreed", "parent_comment_id": bobs })).await.ok();
    let carols = app.comment(&carol, post, "lovely").await;
    app.post(&bob, &format!("/posts/{post}/comments/{carols}/like"), json!({})).await.ok();
    app.post(&bob, "/users/alice/follow", json!({})).await.ok();
    app.post(&carol, "/users/bob/follow", json!({})).await.ok();
    assert_eq!(post_counts(&app, post).await, (1, 3));

    app.delete_account(&bob).await.ok();

    assert_eq!(post_counts(&app, post).await, (0, 1), "his like, his comment and the reply under it");
    assert_eq!(app.scalar(&format!("SELECT like_count FROM comments WHERE id = '{carols}'")).await.as_deref(), Some("0"));
    assert_eq!(stats(&app, &alice, "alice").await.0, 0, "alice's followers");
    assert_eq!(stats(&app, &carol, "carol").await.1, 0, "carol's following");
    // And every counter matches its rows.
    assert_eq!(
        app.count(
            "SELECT 1 FROM posts p WHERE like_count != (SELECT COUNT(*) FROM likes l WHERE l.post_id = p.id) \
             OR comment_count != (SELECT COUNT(*) FROM comments c WHERE c.post_id = p.id)"
        )
        .await,
        0
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn a_block_hides_both_sides_everywhere_and_ends_follow_requests(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (alice, bob, carol) = (app.register("alice").await, app.register("bob").await, app.register("carol").await);
    let alices = app.upload(&alice, "alice's post").await.to_string();
    let bobs = app.upload(&bob, "bob's post").await.to_string();
    let discovery = |user: &User| {
        let (app, user) = (&app, user.clone());
        async move {
            let feed = app.get(&user, "/feed/discovery").await.ok().json();
            feed["data"].as_array().unwrap().iter().map(|p| p["id"].as_str().unwrap().to_string()).collect::<Vec<_>>()
        }
    };

    // Pending requests both ways (both accounts private).
    app.patch(&alice, "/users/me", json!({ "is_private": true })).await.ok();
    app.patch(&bob, "/users/me", json!({ "is_private": true })).await.ok();
    app.post(&alice, "/users/bob/follow", json!({})).await.ok();
    app.post(&bob, "/users/alice/follow", json!({})).await.ok();
    app.patch(&alice, "/users/me", json!({ "is_private": false })).await.ok();
    app.patch(&bob, "/users/me", json!({ "is_private": false })).await.ok();
    assert!(discovery(&bob).await.contains(&alices));

    app.post(&alice, "/users/bob/block", json!({})).await.ok();
    assert!(!discovery(&bob).await.contains(&alices), "the blocked one doesn't see the blocker");
    assert!(!discovery(&alice).await.contains(&bobs), "nor the other way round");
    assert!(discovery(&carol).await.contains(&alices));
    assert_eq!(app.count("SELECT 1 FROM follow_requests").await, 0);
    // A request from before this rule can't be accepted either.
    app.exec(&format!("INSERT INTO follow_requests (requester_id, target_id) VALUES ('{}', '{}')", bob.id, alice.id)).await;
    assert_eq!(app.post(&alice, "/users/me/follow-requests/bob/accept", json!({})).await.status, StatusCode::BAD_REQUEST);

    // No liking each other's comments.
    let comment = app.comment(&alice, carol_post(&app, &carol).await, "on carol's post").await;
    let carols = app.scalar(&format!("SELECT post_id FROM comments WHERE id = '{comment}'")).await.unwrap();
    assert_eq!(
        app.post(&bob, &format!("/posts/{carols}/comments/{comment}/like"), json!({})).await.status,
        StatusCode::BAD_REQUEST
    );
}

async fn carol_post(app: &TestApp, carol: &User) -> Uuid {
    app.upload(carol, "carol's post").await
}
