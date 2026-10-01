//! The interaction log for ranking Discovery (handlers/events.rs): what it
//! records, the opt-out that deletes it, the export, the account deletion
//! and the retention period.

use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use super::support::*;
use crate::retention;

async fn events(app: &TestApp, user: &User) -> String {
    app.scalar(&format!(
        "SELECT string_agg(event_type, ',' ORDER BY created_at, id) FROM post_events WHERE user_id = '{}'",
        user.id
    ))
    .await
    .unwrap_or_default()
}

async fn toggle_like(app: &TestApp, user: &User, post: Uuid) {
    app.post(user, &format!("/posts/{post}/like"), json!({})).await.ok();
}

#[sqlx::test(migrations = "./migrations")]
async fn likes_and_comments_are_logged_where_they_happen(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (alice, bob) = (app.register("alice").await, app.register("bob").await);
    let post = app.upload(&alice, "Sunset").await;

    toggle_like(&app, &bob, post).await;
    toggle_like(&app, &bob, post).await;
    let comment = app.comment(&bob, post, "Lovely").await;
    let comment_like = format!("/posts/{post}/comments/{comment}/like");
    app.post(&bob, &comment_like, json!({})).await.ok();
    app.post(&bob, &comment_like, json!({})).await.ok();

    assert_eq!(events(&app, &bob).await, "like,unlike,comment,comment_like,comment_unlike");
    assert_eq!(app.count(&format!("SELECT 1 FROM post_events WHERE post_id != '{post}'")).await, 0);
    assert_eq!(app.get(&bob, "/users/me").await.ok().json()["personalization_enabled"], true);
    // Only the owner sees the setting.
    assert!(app.get(&alice, "/users/bob").await.ok().json().get("personalization_enabled").is_none());
}

#[sqlx::test(migrations = "./migrations")]
async fn switching_personalization_off_deletes_the_log_and_stops_it(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (alice, bob) = (app.register("alice").await, app.register("bob").await);
    let post = app.upload(&alice, "Sunset").await;
    toggle_like(&app, &bob, post).await;
    toggle_like(&app, &alice, post).await;

    // Allowed while suspended: it's an objection (Art. 21 GDPR).
    app.exec(&format!("UPDATE users SET suspended_until = NOW() + INTERVAL '7 days' WHERE id = '{}'", bob.id)).await;
    let off = app.patch(&bob, "/users/me/personalization", json!({ "enabled": false })).await.ok().json();
    assert_eq!(off["enabled"], false);
    assert_eq!(events(&app, &bob).await, "");
    assert_eq!(events(&app, &alice).await, "like", "other accounts' logs stay");
    assert_eq!(app.get(&bob, "/users/me").await.ok().json()["personalization_enabled"], false);

    app.exec(&format!("UPDATE users SET suspended_until = NULL WHERE id = '{}'", bob.id)).await;
    toggle_like(&app, &bob, post).await;
    app.comment(&bob, post, "Still here").await;
    assert_eq!(events(&app, &bob).await, "", "nothing is logged while it's off");

    app.patch(&bob, "/users/me/personalization", json!({ "enabled": true })).await.ok();
    toggle_like(&app, &bob, post).await;
    assert_eq!(events(&app, &bob).await, "like");
}

#[sqlx::test(migrations = "./migrations")]
async fn the_export_lists_the_log_and_deleting_the_account_deletes_it(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (alice, bob) = (app.register("alice").await, app.register("bob").await);
    let post = app.upload(&alice, "Sunset").await;
    toggle_like(&app, &bob, post).await;

    let export = app.export(&bob).await;
    assert_eq!(export["profile"]["personalization_enabled"], true);
    let logged = export["discovery_interactions"].as_array().unwrap();
    assert_eq!(logged.len(), 1);
    assert_eq!(logged[0]["post_id"], post.to_string());
    assert_eq!(logged[0]["event"], "like");

    app.delete_account(&bob).await.ok();
    assert_eq!(app.count("SELECT 1 FROM post_events").await, 0);
}

#[sqlx::test(migrations = "./migrations")]
async fn retention_drops_months_past_twelve_and_prepares_the_next_ones(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (alice, bob) = (app.register("alice").await, app.register("bob").await);
    let post = app.upload(&alice, "Sunset").await;

    // A month whose entries are all past the period, as its own partition;
    // in the default partition one entry past it and one within it.
    app.exec(
        r#"
        DO $$
        DECLARE m DATE := (date_trunc('month', NOW()) - INTERVAL '14 months')::date;
        BEGIN
            EXECUTE format('CREATE TABLE post_events_%s PARTITION OF post_events FOR VALUES FROM (%L) TO (%L)',
                           to_char(m, 'YYYY_MM'), m, m + INTERVAL '1 month');
        END $$
        "#,
    )
    .await;
    for age in ["14 months", "13 months", "11 months"] {
        app.exec(&format!(
            "INSERT INTO post_events (user_id, post_id, event_type, created_at) VALUES ('{}', '{post}', 'like', NOW() - INTERVAL '{age}')",
            bob.id
        ))
        .await;
    }
    let old_partition = app
        .scalar("SELECT 'post_events_' || to_char(date_trunc('month', NOW()) - INTERVAL '14 months', 'YYYY_MM')")
        .await
        .unwrap();

    retention::sweep(&app.state).await;

    assert_eq!(app.scalar(&format!("SELECT to_regclass('{old_partition}')")).await, None, "expired month dropped");
    assert_eq!(app.count("SELECT 1 FROM post_events").await, 1, "only the entry within 12 months is left");
    assert_eq!(
        app.count("SELECT 1 FROM post_events WHERE created_at > NOW() - INTERVAL '12 months'").await,
        1
    );
    let ahead = app
        .scalar("SELECT 'post_events_' || to_char(date_trunc('month', NOW()) + INTERVAL '3 months', 'YYYY_MM')")
        .await
        .unwrap();
    assert!(app.scalar(&format!("SELECT to_regclass('{ahead}')")).await.is_some(), "partitions ready three months ahead");
}
