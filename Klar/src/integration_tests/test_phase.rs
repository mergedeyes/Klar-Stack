//! Test-phase opt-in to keep an account through the pre-launch wipe
//! (handlers/test_phase.rs).

use serde_json::json;
use sqlx::PgPool;

use super::support::*;

#[sqlx::test(migrations = "./migrations")]
async fn keeping_the_account_is_opt_in_and_reversible(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let tester = app.register("tester").await;

    let status = app.get(&tester, "/users/me/keep-account").await.ok().json();
    assert_eq!(status["keep"], false, "off unless asked for");

    let on = app.patch(&tester, "/users/me/keep-account", json!({ "keep": true })).await.ok().json();
    assert_eq!(on["keep"], true);
    let first = on["since"].as_str().unwrap().to_string();
    let again = app.patch(&tester, "/users/me/keep-account", json!({ "keep": true })).await.ok().json();
    assert_eq!(again["since"], first.as_str(), "opting in twice keeps the first date");
    assert_eq!(app.export(&tester).await["profile"]["keep_after_test_at"], first.as_str());

    let off = app.patch(&tester, "/users/me/keep-account", json!({ "keep": false })).await.ok().json();
    assert_eq!((off["keep"].as_bool(), off["since"].is_null()), (Some(false), true));
    assert_eq!(app.count("SELECT 1 FROM users WHERE keep_after_test_at IS NOT NULL").await, 0);
}
