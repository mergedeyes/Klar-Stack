//! A database failure reaches the client as a generic message, never the
//! database's own text (errors.rs, utils::DbResultExt). Closing the pool
//! makes every query fail, in the middleware that runs before each handler
//! as much as in the handlers.

use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use super::support::*;

#[sqlx::test(migrations = "./migrations")]
async fn a_database_failure_says_nothing_about_the_database(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let alice = app.register("alice").await;
    app.state.db.close().await;

    // The session check (account_lock::enforce_session) runs first for a
    // signed-in request; an anonymous one reaches the handler.
    for res in [
        app.get(&alice, "/users/me").await,
        app.post(&alice, "/posts", json!({ "caption": "hello" })).await,
        app.anon_get("/users/alice").await,
    ] {
        assert_eq!(res.status, StatusCode::INTERNAL_SERVER_ERROR);
        let error = res.json()["error"].as_str().unwrap().to_string();
        assert_eq!(error, "Database error", "the database's own message must not reach the client");
    }
}
