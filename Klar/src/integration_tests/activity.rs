//! Settings -> Your activity (handlers/activity.rs): an account's own likes
//! and comments, newest first, limited to posts it can still see.

use serde_json::{json, Value};
use sqlx::PgPool;

use super::support::*;

fn ids(res: Value, key: &str) -> Vec<String> {
    res.as_array().unwrap().iter().map(|item| item[key].as_str().unwrap().to_string()).collect()
}

#[sqlx::test(migrations = "./migrations")]
async fn activity_lists_own_likes_and_comments_on_posts_still_visible(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (alice, bob, carol) = (app.register("alice").await, app.register("bob").await, app.register("carol").await);
    let bobs = app.upload(&bob, "bob's post").await;
    let carols = app.upload(&carol, "carol's post").await;
    let own = app.upload(&alice, "alice's post").await;

    for post in [bobs, own, carols] {
        app.post(&alice, &format!("/posts/{post}/like"), json!({})).await.ok();
    }
    // Someone else's likes and comments never show up.
    app.post(&bob, &format!("/posts/{own}/like"), json!({})).await.ok();
    app.comment(&bob, own, "bob's comment").await;
    let on_bobs = app.comment(&alice, bobs, "first").await;
    let on_carols = app.comment(&alice, carols, "second").await;

    // Newest first, by when the like happened rather than the post's age.
    let likes = app.get(&alice, "/users/me/activity/likes").await.ok().json();
    assert_eq!(ids(likes.clone(), "id"), vec![carols.to_string(), own.to_string(), bobs.to_string()]);
    assert_eq!(likes[0]["username"], "carol");
    assert!(likes[0]["liked_at"].is_string());
    let comments = app.get(&alice, "/users/me/activity/comments").await.ok().json();
    assert_eq!(ids(comments.clone(), "id"), vec![on_carols.to_string(), on_bobs.to_string()]);
    assert_eq!(comments[0]["post_username"], "carol");
    assert_eq!(comments[0]["body"], "second");

    // Pages continue from the last item's time and id.
    let first = app.get(&alice, "/users/me/activity/likes?limit=1").await.ok().json();
    let next = format!(
        "/users/me/activity/likes?limit=5&cursor={}&cursor_id={}",
        urlencoding(first[0]["liked_at"].as_str().unwrap()),
        first[0]["id"].as_str().unwrap(),
    );
    assert_eq!(ids(app.get(&alice, &next).await.ok().json(), "id"), vec![own.to_string(), bobs.to_string()]);

    // A block takes the other account's posts out of both lists, and a
    // comment removed by moderation is gone from them too.
    app.post(&carol, "/users/alice/block", json!({})).await.ok();
    app.exec(&format!("UPDATE comments SET moderation_status = 'removed' WHERE id = '{on_bobs}'")).await;
    assert_eq!(ids(app.get(&alice, "/users/me/activity/likes").await.ok().json(), "id"), vec![own.to_string(), bobs.to_string()]);
    assert!(ids(app.get(&alice, "/users/me/activity/comments").await.ok().json(), "id").is_empty());

    // So does a private account the caller doesn't follow.
    app.patch(&bob, "/users/me", json!({ "is_private": true })).await.ok();
    assert_eq!(ids(app.get(&alice, "/users/me/activity/likes").await.ok().json(), "id"), vec![own.to_string()]);
}

fn urlencoding(value: &str) -> String {
    value.replace(':', "%3A").replace('+', "%2B")
}
