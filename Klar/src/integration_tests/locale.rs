//! The account's language (users::set_locale): given at registration,
//! filled in later for older accounts, changed in Settings, and part of the
//! data export.

use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use super::support::*;

#[sqlx::test(migrations = "./migrations")]
async fn the_language_is_kept_with_the_account(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let res = app
        .anon_post(
            "/auth/register",
            json!({ "username": "lena", "email": "lena@example.test", "password": PASSWORD, "accept_terms": true, "locale": "de" }),
        )
        .await
        .ok()
        .json();
    assert_eq!(res["user"]["locale"], "de", "the sign-up's language is the account's");

    // An account from before languages existed has none until the app sets it.
    let older = app.register("older").await;
    assert!(app.get(&older, "/users/me").await.ok().json().get("locale").is_none());
    app.patch(&older, "/users/me/locale", json!({ "locale": "de" })).await.ok();
    assert_eq!(app.get(&older, "/users/me").await.ok().json()["locale"], "de");
    assert_eq!(app.export(&older).await["profile"]["locale"], "de");

    // Only languages Klar has.
    assert_eq!(app.patch(&older, "/users/me/locale", json!({ "locale": "fr" })).await.status, StatusCode::BAD_REQUEST);
    let other = app.register("other").await;
    assert!(app.get(&other, "/users/older").await.ok().json().get("locale").is_none(), "only the owner sees it");
    assert_eq!(
        app.anon_post(
            "/auth/register",
            json!({ "username": "max", "email": "max@example.test", "password": PASSWORD, "accept_terms": true, "locale": "xx" }),
        )
        .await
        .status,
        StatusCode::BAD_REQUEST
    );
}
