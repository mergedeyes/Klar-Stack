//! The account's language (users::set_locale): given at registration,
//! filled in later for older accounts, changed in Settings, and part of the
//! data export.

use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
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

/// A request as the frontend sends it: with the page's language.
async fn send_in(app: &TestApp, lang: Option<&str>, method: Method, path: &str, body: Option<serde_json::Value>) -> Resp {
    let mut req = Request::builder().method(method).uri(path);
    if let Some(lang) = lang {
        req = req.header(header::ACCEPT_LANGUAGE, lang);
    }
    let req = match body {
        Some(body) => req.header(header::CONTENT_TYPE, "application/json").body(Body::from(body.to_string())),
        None => req.body(Body::empty()),
    }
    .unwrap();
    app.send(req, "10.250.0.1").await
}

#[sqlx::test(migrations = "./migrations")]
async fn errors_come_in_the_language_the_page_asks_for(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let missing = format!("/posts/{}", uuid::Uuid::new_v4());

    let res = send_in(&app, Some("de-DE,de;q=0.9,en;q=0.8"), Method::GET, &missing, None).await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);
    assert_eq!(res.json()["error"], "Beitrag nicht gefunden");
    // English when asked for, and when nothing is asked for.
    assert_eq!(send_in(&app, Some("en-US"), Method::GET, &missing, None).await.json()["error"], "Post not found");
    assert_eq!(send_in(&app, None, Method::GET, &missing, None).await.json()["error"], "Post not found");

    // Messages with values in them, and field names inside them.
    let short = json!({ "username": "x", "email": "x@example.test", "password": PASSWORD, "accept_terms": true });
    let res = send_in(&app, Some("de"), Method::POST, "/auth/register", Some(short)).await;
    assert_eq!(res.json()["error"], "Der Nutzername muss zwischen 3 und 30 Zeichen lang sein");
    let res = send_in(&app, Some("de"), Method::GET, "/users/nobody_here", None).await;
    assert_eq!(res.json()["error"], "Konto „nobody_here“ nicht gefunden");
}
