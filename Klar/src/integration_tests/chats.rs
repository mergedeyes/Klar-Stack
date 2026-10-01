//! Direct messages (handlers/chats.rs): only between mutual followers, only
//! the two participants can read or react, replies stay in their own
//! conversation, and a deleted account's messages leave the partner's chat.

use axum::http::StatusCode;
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use super::support::*;

async fn befriend(app: &TestApp, a: &User, a_name: &str, b: &User, b_name: &str) {
    app.post(a, &format!("/users/{b_name}/follow"), json!({})).await.ok();
    app.post(b, &format!("/users/{a_name}/follow"), json!({})).await.ok();
}

async fn send(app: &TestApp, from: &User, to: &User, body: &str) -> Value {
    app.post(from, "/chats/send", json!({ "receiver_id": to.id, "body": body })).await.ok().json()
}

#[sqlx::test(migrations = "./migrations")]
async fn messages_go_only_between_mutual_followers_and_are_validated(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (alice, bob, carol) = (app.register("alice").await, app.register("bob").await, app.register("carol").await);
    let to = |user: &User, body: &str| json!({ "receiver_id": user.id, "body": body });

    // One-sided follows aren't enough.
    app.post(&alice, "/users/bob/follow", json!({})).await.ok();
    assert_eq!(app.post(&alice, "/chats/send", to(&bob, "hi")).await.status, StatusCode::FORBIDDEN);
    app.post(&bob, "/users/alice/follow", json!({})).await.ok();

    assert_eq!(app.post(&alice, "/chats/send", to(&alice, "me")).await.status, StatusCode::BAD_REQUEST);
    assert_eq!(app.post(&alice, "/chats/send", to(&bob, "   ")).await.status, StatusCode::BAD_REQUEST);
    assert_eq!(app.post(&alice, "/chats/send", to(&bob, &"x".repeat(2001))).await.status, StatusCode::BAD_REQUEST);
    let first = send(&app, &alice, &bob, "hi bob").await;

    // A reply can only quote a message of the same conversation.
    befriend(&app, &alice, "alice", &carol, "carol").await;
    let elsewhere = send(&app, &alice, &carol, "hi carol").await;
    let reply = |to: &User, id: &Value| json!({ "receiver_id": to.id, "body": "re", "reply_to_message_id": id });
    assert_eq!(app.post(&alice, "/chats/send", reply(&bob, &elsewhere["id"])).await.status, StatusCode::BAD_REQUEST);
    app.post(&bob, "/chats/send", reply(&alice, &first["id"])).await.ok();
}

#[sqlx::test(migrations = "./migrations")]
async fn only_the_two_participants_can_read_react_edit_or_delete(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (alice, bob, carol) = (app.register("alice").await, app.register("bob").await, app.register("carol").await);
    befriend(&app, &alice, "alice", &bob, "bob").await;
    let message = send(&app, &alice, &bob, "just us").await;
    let (message_id, conversation) = (message["id"].as_str().unwrap(), message["conversation_id"].as_str().unwrap());
    let react = |emoji: &str| json!({ "emoji": emoji });

    // Carol, outside the conversation: no reading, no reacting (404, so
    // message ids can't be probed), no editing or deleting.
    assert_eq!(app.get(&carol, &format!("/chats/{conversation}/messages")).await.status, StatusCode::FORBIDDEN);
    let reactions = format!("/chats/messages/{message_id}/reactions");
    assert_eq!(app.post(&carol, &reactions, react("👍")).await.status, StatusCode::NOT_FOUND);
    let edit = format!("/chats/messages/{message_id}");
    assert_eq!(app.patch(&carol, &edit, json!({ "body": "mine now" })).await.status, StatusCode::FORBIDDEN);
    assert_eq!(app.delete(&carol, &edit).await.status, StatusCode::FORBIDDEN);
    // Bob is in it, but only the sender edits or deletes.
    assert_eq!(app.patch(&bob, &edit, json!({ "body": "changed" })).await.status, StatusCode::FORBIDDEN);
    assert_eq!(app.delete(&bob, &edit).await.status, StatusCode::FORBIDDEN);

    // Reactions: one emoji-sized string, toggled.
    assert_eq!(app.post(&bob, &reactions, react("")).await.status, StatusCode::BAD_REQUEST);
    assert_eq!(app.post(&bob, &reactions, react(&"🎉".repeat(17))).await.status, StatusCode::BAD_REQUEST);
    app.post(&bob, &reactions, react("👍")).await.ok();
    let messages = app.get(&alice, &format!("/chats/{conversation}/messages")).await.ok().json();
    assert_eq!(messages[0]["reactions"][0]["emoji"], "👍");
    app.post(&bob, &reactions, react("👍")).await.ok();
    let messages = app.get(&alice, &format!("/chats/{conversation}/messages")).await.ok().json();
    assert_eq!(messages[0]["reactions"], json!([]));

    app.patch(&alice, &edit, json!({ "body": "edited" })).await.ok();
    let messages = app.get(&bob, &format!("/chats/{conversation}/messages")).await.ok().json();
    assert_eq!(messages[0]["body"], "edited");
    assert!(!messages[0]["edited_at"].is_null());
    app.delete(&alice, &edit).await.ok();
    assert_eq!(app.get(&bob, &format!("/chats/{conversation}/messages")).await.ok().json(), json!([]));
}

#[sqlx::test(migrations = "./migrations")]
async fn a_deleted_accounts_messages_leave_the_partners_chat(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (alice, bob) = (app.register("alice").await, app.register("bob").await);
    befriend(&app, &alice, "alice", &bob, "bob").await;
    send(&app, &alice, &bob, "from alice").await;
    let conversation: Uuid = send(&app, &bob, &alice, "from bob").await["conversation_id"].as_str().unwrap().parse().unwrap();

    app.delete(&bob, "/users/me").await.ok();

    // Alice keeps the conversation with her own messages; bob is "Deleted
    // User" (no id, no name) and his messages are gone.
    let chats = app.get(&alice, "/chats").await.ok().json();
    assert_eq!(chats.as_array().unwrap().len(), 1);
    assert!(chats[0]["other_user_id"].is_null() && chats[0]["other_username"].is_null(), "{chats}");
    let messages = app.get(&alice, &format!("/chats/{conversation}/messages")).await.ok().json();
    let bodies: Vec<_> = messages.as_array().unwrap().iter().map(|m| m["body"].as_str().unwrap()).collect();
    assert_eq!(bodies, vec!["from alice"]);
    // Nobody can message the deleted account.
    assert_eq!(
        app.post(&alice, "/chats/send", json!({ "receiver_id": bob.id, "body": "hello?" })).await.status,
        StatusCode::FORBIDDEN
    );

    // When alice goes too, nothing of the conversation is left.
    app.delete(&alice, "/users/me").await.ok();
    assert_eq!(app.count(&format!("SELECT 1 FROM conversations WHERE id = '{conversation}'")).await, 0);
}
