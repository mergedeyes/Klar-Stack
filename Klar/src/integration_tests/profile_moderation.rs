//! Removing parts of a profile (handlers/profile_moderation.rs): a decision
//! with a strike and a statement, the reports behind it closed, everything
//! back after an accepted objection, and a forced rename.

use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use super::support::*;

#[sqlx::test(migrations = "./migrations")]
async fn parts_of_a_profile_are_removed_and_come_back_after_an_objection(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (alice, bob, admin) = (app.register("alice").await, app.register("bob").await, app.admin().await);
    app.patch(&alice, "/users/me", json!({ "display_name": "Hateful Name", "bio": "Hateful bio" })).await.ok();
    app.upload_avatar(&alice, [200, 10, 10]).await;
    let avatar = app.scalar(&format!("SELECT avatar_url FROM users WHERE id = '{}'", alice.id)).await.unwrap();
    let report = app.report(&bob, "user", alice.id, "hate_speech").await;

    let path = "/admin/users/alice/profile-removal";
    assert_eq!(app.post(&bob, path, json!({ "fields": ["bio"] })).await.status, StatusCode::FORBIDDEN);
    assert_eq!(app.post(&admin, path, json!({ "fields": ["email"] })).await.status, StatusCode::BAD_REQUEST);
    let elsewhere = app.report(&alice, "user", bob.id, "spam").await;
    let wrong = json!({ "fields": ["bio"], "report_ids": [elsewhere] });
    assert_eq!(app.post(&admin, path, wrong).await.status, StatusCode::CONFLICT, "only reports on this account");

    app.post(&admin, path, json!({ "fields": ["avatar", "bio", "display_name"], "report_ids": [report] })).await.ok();

    // Gone from the profile, the picture out of reach but kept.
    let me = app.get(&alice, "/users/me").await.ok().json();
    assert!(me["bio"].is_null() && me["display_name"].is_null() && me["avatar_url"].is_null(), "{me}");
    let kept = app
        .scalar(&format!("SELECT removed_fields->>'avatar_key' FROM moderation_decisions WHERE affected_user_id = '{}'", alice.id))
        .await
        .unwrap();
    eventually("avatar moved", || async { !app.media_file(&avatar).exists() && app.media_file(&kept).exists() }).await;

    // A decision like any removal: a statement listing what went, a
    // strike, the report closed and its reporter told.
    let d = app.get(&alice, "/moderation/decisions").await.ok().json()[0].clone();
    assert_eq!(d["target_type"], "user");
    assert_eq!(d["restriction"], "removed");
    assert!(d["explanation"].as_str().unwrap().contains("Entfernt: Profilbild, Beschreibung, Anzeigename."), "{d}");
    assert!(d["content_excerpt"].as_str().unwrap().contains("Hateful bio"), "{d}");
    assert_eq!(d["can_object"], true);
    assert!(app.get(&alice, "/users/me/standing").await.ok().json()["score"].as_i64().unwrap() > 0);
    assert_eq!(app.scalar(&format!("SELECT outcome FROM reports WHERE id = '{report}'")).await.as_deref(), Some("removed"));

    // An accepted objection puts it all back and takes the strike away.
    let id = d["id"].as_str().unwrap();
    app.post(&alice, &format!("/moderation/decisions/{id}/objection"), json!({ "text": "It's a quote from a novel." })).await.ok();
    app.post(&admin, &format!("/admin/moderation/decisions/{id}/objection"), json!({ "accept": true, "response": "Agreed." }))
        .await
        .ok();
    let me = app.get(&alice, "/users/me").await.ok().json();
    assert_eq!(me["bio"], "Hateful bio");
    assert_eq!(me["display_name"], "Hateful Name");
    assert!(me["avatar_url"].as_str().is_some_and(|url| url.contains(&kept)), "{me}");
    assert_eq!(app.get(&alice, "/users/me/standing").await.ok().json()["score"], 0);
}

#[sqlx::test(migrations = "./migrations")]
async fn an_impersonating_username_is_replaced_and_can_be_chosen_anew(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (fake, bob, admin) = (app.register("real_journalist").await, app.register("bob").await, app.admin().await);
    let report = app.report(&bob, "user", fake.id, "impersonation").await;
    app.post(&admin, "/admin/users/real_journalist/profile-removal", json!({ "fields": ["username"], "report_ids": [report] }))
        .await
        .ok();

    let name = app.get(&fake, "/users/me").await.ok().json()["username"].as_str().unwrap().to_string();
    assert!(name.starts_with("user_") && name.len() == 13, "{name}");
    let d = app.get(&fake, "/moderation/decisions").await.ok().json()[0].clone();
    assert!(d["explanation"].as_str().unwrap().contains(&name), "{d}");
    // A new name right away, without the 14-day wait.
    app.patch(&fake, "/users/me", json!({ "username": "someone_else" })).await.ok();
    assert_eq!(app.anon_get("/users/real_journalist").await.status, StatusCode::NOT_FOUND);

    // Without a report, the team picks the violation itself.
    let path = "/admin/users/someone_else/profile-removal";
    assert_eq!(app.post(&admin, path, json!({ "fields": ["username"] })).await.status, StatusCode::BAD_REQUEST);
    app.post(&admin, path, json!({ "fields": ["username"], "violation": "impersonation_deceptive" })).await.ok();
    let d = app.get(&fake, "/moderation/decisions").await.ok().json()[0].clone();
    assert_eq!(d["source"], "own_initiative");
}
