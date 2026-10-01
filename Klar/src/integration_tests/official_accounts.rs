//! Renaming official accounts to staff names (handlers/official_accounts.rs).

use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use super::support::*;

#[sqlx::test(migrations = "./migrations")]
async fn an_admin_gives_an_official_account_a_staff_name(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let admin = app.admin().await;
    let bob = app.register("bob").await;
    // Registering with a staff name is refused, so the account starts
    // under a temporary one.
    let refused = app
        .anon_post(
            "/auth/register",
            json!({ "username": "Klar", "email": "kontakt@klarsocial.eu", "password": PASSWORD, "accept_terms": true }),
        )
        .await;
    assert_eq!(refused.status, StatusCode::BAD_REQUEST);
    let official = app.register_with_email("klar_team", "kontakt@klarsocial.eu").await;
    // Lookalike addresses are never official.
    app.register_with_email("fake_one", "kontakt@mail.klarsocial.eu").await;
    app.register_with_email("fake_two", "kontakt@klarsocial.eu.example.test").await;

    let rename = format!("/admin/official-accounts/{}/username", official.id);
    let body = |username: &str| json!({ "username": username, "reason": "The official profile" });

    // Only verified addresses count: whoever registers one without the
    // inbox can't be renamed.
    assert_eq!(app.get(&admin, "/admin/official-accounts").await.ok().json()["accounts"], json!([]));
    assert_eq!(app.post(&admin, &rename, body("Klar")).await.status, StatusCode::NOT_FOUND);
    app.exec("UPDATE users SET email_verified = TRUE").await;
    let accounts = app.get(&admin, "/admin/official-accounts").await.ok().json()["accounts"].clone();
    assert_eq!(accounts.as_array().unwrap().len(), 1, "{accounts}");
    assert_eq!(accounts[0]["username"], "klar_team");

    // Admins only, with a reason, and never a route name.
    assert_eq!(app.post(&bob, &rename, body("Klar")).await.status, StatusCode::FORBIDDEN);
    assert_eq!(app.get(&bob, "/admin/official-accounts").await.status, StatusCode::FORBIDDEN);
    let no_reason = json!({ "username": "Klar", "reason": " " });
    assert_eq!(app.post(&admin, &rename, no_reason).await.status, StatusCode::BAD_REQUEST);
    assert_eq!(app.post(&admin, &rename, body("me")).await.status, StatusCode::BAD_REQUEST);
    assert_eq!(app.post(&admin, &rename, body("BOB")).await.status, StatusCode::CONFLICT);
    // Other accounts aren't official, whatever their name.
    let bob_rename = format!("/admin/official-accounts/{}/username", bob.id);
    assert_eq!(app.post(&admin, &bob_rename, body("Klar")).await.status, StatusCode::NOT_FOUND);

    app.post(&admin, &rename, body("Klar")).await.ok();
    assert_eq!(app.get(&bob, "/users/klar").await.ok().json()["username"], "Klar");
    assert_eq!(app.post(&admin, &rename, body("Klar")).await.status, StatusCode::BAD_REQUEST);

    let renames = app.get(&admin, "/admin/official-accounts").await.ok().json()["renames"].clone();
    assert_eq!(renames.as_array().unwrap().len(), 1);
    assert_eq!(renames[0]["old_username"], "klar_team");
    assert_eq!(renames[0]["new_username"], "Klar");
    assert_eq!(renames[0]["reason"], "The official profile");
    assert_eq!(renames[0]["renamed_by"], "site_admin");

    // The account itself still can't pick another staff name.
    app.exec("UPDATE users SET username_changed_at = NULL").await;
    let own = app.patch(&official, "/users/me", json!({ "username": "support" })).await;
    assert_eq!(own.status, StatusCode::BAD_REQUEST);
}
