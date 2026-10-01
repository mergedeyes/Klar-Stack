//! What goes after its retention period (retention.rs): accounts that never
//! verified their address, reports, decisions, notifications and expired
//! refresh tokens -- each only once nothing rests on it any more.

use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use super::support::*;
use crate::retention;

async fn exists(app: &TestApp, table: &str, id: Uuid) -> bool {
    app.count(&format!("SELECT 1 FROM {table} WHERE id = '{id}'")).await == 1
}

#[sqlx::test(migrations = "./migrations")]
async fn unverified_accounts_get_a_reminder_and_go_a_week_later(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (old, fresh, verified) =
        (app.register_unverified("old").await, app.register_unverified("fresh").await, app.register("verified").await);
    app.exec(&format!("UPDATE users SET created_at = NOW() - INTERVAL '24 days' WHERE id IN ('{}', '{}')", old.id, verified.id))
        .await;

    // A week before the deletion: a reminder with a fresh link, valid as
    // long. Only once.
    retention::sweep_unverified(&app.state).await;
    let reminded = |user: &User| format!("SELECT verification_reminder_at FROM users WHERE id = '{}'", user.id);
    let first = app.scalar(&reminded(&old)).await.expect("reminded");
    assert!(app.scalar(&reminded(&fresh)).await.is_none());
    assert!(app.scalar(&reminded(&verified)).await.is_none());
    assert_eq!(
        app.count(&format!(
            "SELECT 1 FROM email_tokens WHERE user_id = '{}' AND token_type = 'verification' AND used_at IS NULL \
             AND expires_at > NOW() + INTERVAL '6 days'",
            old.id
        ))
        .await,
        1
    );
    retention::sweep_unverified(&app.state).await;
    assert_eq!(app.scalar(&reminded(&old)).await, Some(first));

    // Thirty days old, but the reminder is only a day old: still waits.
    app.exec(&format!("UPDATE users SET created_at = NOW() - INTERVAL '31 days' WHERE id IN ('{}', '{}')", old.id, verified.id))
        .await;
    retention::sweep_unverified(&app.state).await;
    assert!(exists(&app, "users", old.id).await);

    // A week after the reminder: gone. The others stay.
    app.exec(&format!("UPDATE users SET verification_reminder_at = NOW() - INTERVAL '8 days' WHERE id = '{}'", old.id)).await;
    retention::sweep_unverified(&app.state).await;
    assert!(!exists(&app, "users", old.id).await);
    assert!(exists(&app, "users", fresh.id).await && exists(&app, "users", verified.id).await);
}

#[sqlx::test(migrations = "./migrations")]
async fn records_go_once_nothing_rests_on_them(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (alice, bob, admin) = (app.register("alice").await, app.register("bob").await, app.admin().await);

    // Three spam reports: dismissed, actioned (a strike for alice), pending.
    let dismissed = app.report(&bob, "post", app.upload(&alice, "one").await, "spam").await;
    let removed_post = app.upload(&alice, "two").await;
    let actioned = app.report(&bob, "post", removed_post, "spam").await;
    let pending = app.report(&bob, "post", app.upload(&alice, "three").await, "spam").await;
    app.post(&admin, &format!("/admin/reports/{dismissed}/dismiss"), json!({})).await.ok();
    app.post(&admin, &format!("/admin/reports/{actioned}/remove"), json!({})).await.ok();
    let decision: Uuid = app
        .scalar(&format!("SELECT id FROM moderation_decisions WHERE target_id = '{removed_post}'"))
        .await
        .unwrap()
        .parse()
        .unwrap();

    // Six and a half months later: the dismissed report goes; the one
    // behind the active strike stays, and so does the pending one.
    app.exec("UPDATE reports SET created_at = NOW() - INTERVAL '200 days', reviewed_at = reviewed_at - INTERVAL '200 days'").await;
    app.exec(&format!("UPDATE moderation_decisions SET created_at = NOW() - INTERVAL '200 days' WHERE id = '{decision}'")).await;
    retention::sweep(&app.state).await;
    assert!(!exists(&app, "reports", dismissed).await);
    assert!(exists(&app, "reports", actioned).await, "an active strike rests on it");
    assert!(exists(&app, "reports", pending).await);
    assert!(!exists(&app, "posts", removed_post).await, "removed content went after the objection window");

    // Once the strike has expired, the report goes too; the decision is
    // kept three years.
    app.exec("UPDATE account_strikes SET expires_at = NOW() - INTERVAL '1 second'").await;
    retention::sweep(&app.state).await;
    assert!(!exists(&app, "reports", actioned).await);
    assert!(exists(&app, "moderation_decisions", decision).await);
    app.exec(&format!("UPDATE moderation_decisions SET created_at = NOW() - INTERVAL '4 years' WHERE id = '{decision}'")).await;
    retention::sweep(&app.state).await;
    assert!(!exists(&app, "moderation_decisions", decision).await);
    assert_eq!(
        app.count(&format!("SELECT 1 FROM notifications WHERE decision_id = '{decision}'")).await,
        0,
        "its notices go with it"
    );

    // Notifications: read ones after 90 days, unread ones after a year.
    for name in ["carol", "dave", "erin"] {
        let user = app.register(name).await;
        app.post(&user, "/users/alice/follow", json!({})).await.ok();
    }
    let follow_from = |name: &str| {
        format!("SELECT n.id FROM notifications n JOIN users u ON u.id = n.actor_id WHERE u.username = '{name}'")
    };
    app.exec(&format!("UPDATE notifications SET is_read = TRUE, created_at = NOW() - INTERVAL '100 days' WHERE id IN ({})", follow_from("carol"))).await;
    app.exec(&format!("UPDATE notifications SET created_at = NOW() - INTERVAL '100 days' WHERE id IN ({})", follow_from("dave"))).await;
    app.exec(&format!("UPDATE notifications SET created_at = NOW() - INTERVAL '400 days' WHERE id IN ({})", follow_from("erin"))).await;
    retention::sweep(&app.state).await;
    assert_eq!(app.count(&follow_from("carol")).await, 0);
    assert_eq!(app.count(&follow_from("dave")).await, 1, "unread, and younger than a year");
    assert_eq!(app.count(&follow_from("erin")).await, 0);

    // Expired refresh tokens.
    app.exec(&format!("UPDATE refresh_tokens SET expires_at = NOW() - INTERVAL '1 day' WHERE user_id = '{}'", bob.id)).await;
    retention::sweep(&app.state).await;
    assert_eq!(app.count(&format!("SELECT 1 FROM refresh_tokens WHERE user_id = '{}'", bob.id)).await, 0);
    assert!(app.count("SELECT 1 FROM refresh_tokens").await > 0, "the others' are still valid");
}
