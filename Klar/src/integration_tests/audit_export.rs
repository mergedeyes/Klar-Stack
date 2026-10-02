//! The audit export (handlers/audit_export.rs): what it contains, what it
//! leaves out, pseudonyms and identities, and the log of every export.

use std::collections::HashMap;
use std::io::{Cursor, Read};

use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use super::support::*;

/// The export's files by name.
async fn export(app: &TestApp, admin: &User, body: serde_json::Value) -> HashMap<String, String> {
    let res = app.post(admin, "/admin/audit-exports", body).await.ok();
    assert_eq!(res.headers["content-type"], "application/zip");
    let mut zip = zip::ZipArchive::new(Cursor::new(res.body.to_vec())).unwrap();
    let mut files = HashMap::new();
    for i in 0..zip.len() {
        let mut file = zip.by_index(i).unwrap();
        let mut text = String::new();
        file.read_to_string(&mut text).unwrap();
        files.insert(file.name().to_string(), text);
    }
    files
}

fn today() -> String {
    chrono::Utc::now().date_naive().to_string()
}

#[sqlx::test(migrations = "./migrations")]
async fn the_export_lists_the_period_pseudonymised_and_without_content(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (alice, bob, admin) = (app.register("alice").await, app.trusted("bob").await, app.admin().await);
    let post = app.upload(&alice, "Secret caption text").await;
    let report = app.report(&bob, "post", post, "spam").await;
    app.post(&admin, &format!("/admin/reports/{report}/remove"), json!({ "note": "=cmd|' /C calc'!A0" })).await.ok();

    let files = export(&app, &admin, json!({ "from": today(), "to": today(), "reason": "Auskunftsersuchen BNetzA, Az. 123" })).await;
    for name in ["README.txt", "summary.csv", "entscheidungen.csv", "meldungen.csv", "beweissicherung.csv", "exporte.csv"] {
        assert!(files.contains_key(name), "{name} missing");
    }
    let everything = files.values().cloned().collect::<Vec<_>>().join("\n");
    assert!(!everything.contains("Secret caption text"), "no content");
    assert!(!everything.contains("@example") && !everything.contains("alice@"), "no email addresses");
    assert!(!everything.contains("@alice") && !everything.contains("@bob"), "accounts are pseudonymised");
    assert!(!everything.contains(&post.to_string()), "items are pseudonymised");
    assert!(everything.contains("@site_admin"), "the team appears by name");

    let decisions = &files["entscheidungen.csv"];
    assert!(decisions.starts_with('\u{feff}'), "BOM for Excel");
    let row = decisions.lines().nth(1).unwrap();
    assert!(row.contains(";post;I-") && row.contains(";K-"), "{row}");
    assert!(row.contains(";removed;"), "{row}");
    // The same account has the same code in every file of one export.
    let author = row.split(';').find(|c| c.starts_with("K-")).unwrap();
    assert!(files["meldungen.csv"].contains(&format!(";post;{}", row.split(';').find(|c| c.starts_with("I-")).unwrap())));
    assert!(!files["meldungen.csv"].contains(author), "the reporter isn't the author");
    // A note can't run as a formula in Excel.
    assert!(files["meldungen.csv"].contains("'=cmd"), "{}", files["meldungen.csv"]);

    assert!(files["summary.csv"].contains("Meldungen;eingegangen;1\r\n"), "{}", files["summary.csv"]);
    assert!(files["summary.csv"].contains("Entscheidungen;durch das Team;1\r\n"));
    assert!(files["README.txt"].contains("pseudonymisiert") && files["README.txt"].contains("Az. 123"));
    assert!(files["exporte.csv"].contains("Auskunftsersuchen BNetzA, Az. 123"), "the export lists itself");
    assert_eq!(app.count("SELECT 1 FROM audit_exports WHERE with_identities = FALSE").await, 1);

    // A second export uses other codes.
    let again = export(&app, &admin, json!({ "from": today(), "to": today(), "reason": "Gegenprobe" })).await;
    assert!(!again["entscheidungen.csv"].contains(author));
}

#[sqlx::test(migrations = "./migrations")]
async fn identities_show_usernames_and_the_log_says_so(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (alice, bob, admin) = (app.register("alice").await, app.trusted("bob").await, app.admin().await);
    let post = app.upload(&alice, "Secret caption text").await;
    app.report(&bob, "post", post, "spam").await;

    let files = export(&app, &admin, json!({ "from": today(), "to": today(), "with_identities": true, "reason": "Staatsanwaltschaft, Az. 9" })).await;
    assert!(files["meldungen.csv"].contains("@bob"));
    assert!(files["meldungen.csv"].contains(&post.to_string()));
    assert!(!files.values().any(|f| f.contains("Secret caption text")), "still no content");
    assert!(files["README.txt"].contains("mit Identitäten"));
    assert_eq!(app.count("SELECT 1 FROM audit_exports WHERE with_identities AND reason = 'Staatsanwaltschaft, Az. 9'").await, 1);
    let listed = app.get(&admin, "/admin/audit-exports").await.ok().json();
    assert_eq!(listed[0]["with_identities"], true);
    assert_eq!(listed[0]["exported_by_username"], "site_admin");
}

#[sqlx::test(migrations = "./migrations")]
async fn exports_need_an_admin_a_reason_and_a_past_period(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (bob, admin) = (app.register("bob").await, app.admin().await);
    let ok = json!({ "from": today(), "to": today(), "reason": "Prüfung" });
    assert_eq!(app.post(&bob, "/admin/audit-exports", ok.clone()).await.status, StatusCode::FORBIDDEN);
    assert_eq!(app.get(&bob, "/admin/audit-exports").await.status, StatusCode::FORBIDDEN);
    let no_reason = json!({ "from": today(), "to": today(), "reason": " " });
    assert_eq!(app.post(&admin, "/admin/audit-exports", no_reason).await.status, StatusCode::BAD_REQUEST);
    let backwards = json!({ "from": today(), "to": "2026-01-01", "reason": "Prüfung" });
    assert_eq!(app.post(&admin, "/admin/audit-exports", backwards).await.status, StatusCode::BAD_REQUEST);
    let future = json!({ "from": today(), "to": "2999-01-01", "reason": "Prüfung" });
    assert_eq!(app.post(&admin, "/admin/audit-exports", future).await.status, StatusCode::BAD_REQUEST);
    assert_eq!(app.count("SELECT 1 FROM audit_exports").await, 0, "refused exports aren't logged");
}
