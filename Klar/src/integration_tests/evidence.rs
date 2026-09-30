//! Evidence preservation (evidence.rs, handlers/evidence.rs): capture on a
//! likely-illegal report, a version per edit, survival of every kind of
//! deletion, the decision rules, the sweeper, access control and the audit
//! trail.

use axum::http::StatusCode;
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

use super::support::*;
use crate::evidence;

async fn record_for(app: &TestApp, target_id: Uuid) -> Option<String> {
    app.scalar(&format!(
        "SELECT id FROM evidence_records WHERE target_id = '{target_id}' ORDER BY created_at DESC LIMIT 1"
    ))
    .await
}

async fn versions(app: &TestApp, evidence_id: &str) -> String {
    app.scalar(&format!(
        "SELECT string_agg(cause, ',' ORDER BY captured_at, id) FROM evidence_versions WHERE evidence_id = '{evidence_id}'"
    ))
    .await
    .unwrap_or_default()
}

async fn copied_files(app: &TestApp, evidence_id: &str) -> i64 {
    app.count(&format!(
        "SELECT 1 FROM evidence_files WHERE evidence_id = '{evidence_id}' AND copied_at IS NOT NULL"
    ))
    .await
}

async fn wait_copied(app: &TestApp, evidence_id: &str, n: i64) {
    eventually("evidence copy", || async { copied_files(app, evidence_id).await == n }).await;
}

#[sqlx::test(migrations = "./migrations")]
async fn report_captures_state_and_edits_add_versions(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (alice, bob, admin) = (app.register("alice").await, app.register("bob").await, app.admin().await);

    let post = app.upload(&alice, "As reported").await;
    let live_key = app.scalar(&format!("SELECT full_key FROM media_assets WHERE post_id = '{post}'")).await.unwrap();
    let r1 = app.report(&bob, "post", post, "violence").await;
    let ev = record_for(&app, post).await.expect("record opened on report");
    wait_copied(&app, &ev, 1).await;
    assert_eq!(versions(&app, &ev).await, "reported");
    assert_eq!(
        app.scalar(&format!("SELECT content->'post'->>'caption' FROM evidence_versions WHERE evidence_id = '{ev}'")).await.as_deref(),
        Some("As reported")
    );
    // Only the item itself: no comments, likes or reports in the snapshot.
    assert_eq!(
        app.scalar(&format!("SELECT content ?| array['comments','likes','reports'] FROM evidence_versions WHERE evidence_id = '{ev}'")).await.as_deref(),
        Some("false")
    );

    // Files at rest are encrypted, not images.
    let key = app.scalar(&format!("SELECT storage_key FROM evidence_files WHERE evidence_id = '{ev}'")).await.unwrap();
    let stored = std::fs::read(app.evidence_file(&key)).unwrap();
    assert_eq!(&stored[..4], b"KLE1");

    app.patch(&alice, &format!("/posts/{post}"), json!({ "caption": "Edited" })).await.ok();
    assert_eq!(versions(&app, &ev).await, "reported,edited");
    assert_eq!(app.count(&format!("SELECT 1 FROM evidence_files WHERE evidence_id = '{ev}'")).await, 1, "image not copied again");

    let r2 = app.report(&admin, "post", post, "sexual_content").await;
    assert_eq!(record_for(&app, post).await.as_deref(), Some(ev.as_str()), "second report shares the record");
    assert_eq!(
        app.scalar(&format!("SELECT array_to_string(reasons, ',') FROM evidence_records WHERE id = '{ev}'")).await.as_deref(),
        Some("sexual_content,violence")
    );

    // Decided only once the last likely-illegal report is resolved.
    app.post(&admin, &format!("/admin/reports/{r1}/dismiss"), json!({})).await.ok();
    assert_eq!(app.scalar(&format!("SELECT decided_at IS NULL FROM evidence_records WHERE id = '{ev}'")).await.as_deref(), Some("true"));
    app.post(&admin, &format!("/admin/reports/{r2}/dismiss"), json!({})).await.ok();
    assert_eq!(app.scalar(&format!("SELECT decision FROM evidence_records WHERE id = '{ev}'")).await.as_deref(), Some("dismissed"));

    // The sweeper purges it, but leaves the live post's own image alone.
    evidence::sweep(&app.state).await;
    assert_eq!(app.scalar(&format!("SELECT purged_at IS NOT NULL FROM evidence_records WHERE id = '{ev}'")).await.as_deref(), Some("true"));
    assert_eq!(app.count(&format!("SELECT 1 FROM evidence_versions WHERE evidence_id = '{ev}'")).await, 0);
    assert!(!app.evidence_file(&key).exists());
    assert!(app.media_file(&live_key).exists(), "live image must survive the purge");
}

#[sqlx::test(migrations = "./migrations")]
async fn csam_is_copied_before_rotation_and_survives_removal(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (alice, bob, admin) = (app.register("alice").await, app.register("bob").await, app.admin().await);

    let post = app.upload(&alice, "csam").await;
    let original_key = app.scalar(&format!("SELECT full_key FROM media_assets WHERE post_id = '{post}'")).await.unwrap();
    let original = std::fs::read(app.media_file(&original_key)).unwrap();

    let report = app.report(&bob, "post", post, "csam").await;
    let ev = record_for(&app, post).await.unwrap();
    wait_copied(&app, &ev, 1).await;
    let sha = app.scalar(&format!("SELECT sha256 FROM evidence_files WHERE evidence_id = '{ev}'")).await.unwrap();
    assert_eq!(sha, hex::encode(Sha256::digest(&original)), "copy is the original image");
    eventually("key rotation", || async { !app.media_file(&original_key).exists() }).await;

    app.post(&admin, &format!("/admin/reports/{report}/remove"), json!({ "note": "confirmed" })).await.ok();
    assert_eq!(app.scalar(&format!("SELECT deletion_trigger FROM evidence_records WHERE id = '{ev}'")).await.as_deref(), Some("moderation_removal"));
    assert_eq!(versions(&app, &ev).await, "reported", "removal adds no version");
    assert_eq!(
        app.scalar(&format!(
            "SELECT decision || ',' || (retain_until > NOW() + INTERVAL '182 days') FROM evidence_records WHERE id = '{ev}'"
        )).await.as_deref(),
        Some("removed,true")
    );

    // Admin access: non-admins refused, a reason required, everything logged.
    assert_eq!(app.post(&bob, &format!("/admin/evidence/{ev}/open"), json!({ "reason": "x" })).await.status, StatusCode::FORBIDDEN);
    assert_eq!(app.post(&admin, &format!("/admin/evidence/{ev}/open"), json!({ "reason": "  " })).await.status, StatusCode::BAD_REQUEST);
    let detail = app.post(&admin, &format!("/admin/evidence/{ev}/open"), json!({ "reason": "review" })).await.ok().json();
    let file_id = detail["versions"][0]["files"][0]["id"].as_str().unwrap().to_string();
    let file = app.post(&admin, &format!("/admin/evidence/{ev}/files/{file_id}"), json!({ "reason": "confirm" })).await.ok();
    assert_eq!(file.body.as_ref(), original.as_slice(), "decrypts to the original");
    assert_eq!(file.headers["cache-control"], "no-store");

    // A legal hold stops the purge even after the retention period.
    app.post(&admin, &format!("/admin/evidence/{ev}/hold"), json!({ "hold": true, "reason": "BKA request" })).await.ok();
    assert_eq!(app.post(&admin, &format!("/admin/evidence/{ev}/hold"), json!({ "hold": true, "reason": "again" })).await.status, StatusCode::CONFLICT);
    app.post(&admin, &format!("/admin/evidence/{ev}/authority-report"), json!({ "authority": "BKA", "reported_on": "2026-09-30", "reference": "AZ-1" })).await.ok();
    assert_eq!(
        app.post(&admin, &format!("/admin/evidence/{ev}/authority-report"), json!({ "authority": "BKA", "reported_on": "2099-01-01" })).await.status,
        StatusCode::BAD_REQUEST
    );
    app.exec(&format!("UPDATE evidence_records SET retain_until = NOW() - INTERVAL '1 day' WHERE id = '{ev}'")).await;
    evidence::sweep(&app.state).await;
    assert_eq!(app.scalar(&format!("SELECT purged_at IS NULL FROM evidence_records WHERE id = '{ev}'")).await.as_deref(), Some("true"));

    app.post(&admin, &format!("/admin/evidence/{ev}/hold"), json!({ "hold": false, "reason": "closed" })).await.ok();
    evidence::sweep(&app.state).await;
    assert_eq!(app.scalar(&format!("SELECT purged_at IS NOT NULL FROM evidence_records WHERE id = '{ev}'")).await.as_deref(), Some("true"));

    let trail = app.scalar(&format!(
        "SELECT string_agg(action, ',' ORDER BY created_at, id) FROM evidence_events WHERE evidence_id = '{ev}'"
    )).await.unwrap();
    assert_eq!(trail, "created,version_added,content_deleted,decided,viewed,file_viewed,hold_set,authority_report,hold_lifted,purged");
}

#[sqlx::test(migrations = "./migrations")]
async fn spam_reports_and_unreported_edits_copy_nothing(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (alice, bob) = (app.register("alice").await, app.register("bob").await);

    let spam = app.upload(&alice, "Buy now").await;
    app.report(&bob, "post", spam, "spam").await;
    app.delete(&alice, &format!("/posts/{spam}")).await.ok();
    assert_eq!(record_for(&app, spam).await, None);

    let clean = app.upload(&alice, "Clean").await;
    app.patch(&alice, &format!("/posts/{clean}"), json!({ "caption": "Still clean" })).await.ok();
    assert_eq!(record_for(&app, clean).await, None);
}

#[sqlx::test(migrations = "./migrations")]
async fn reported_comment_keeps_context_and_survives_cascade(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (alice, bob) = (app.register("alice").await, app.register("bob").await);

    let post = app.upload(&bob, "Bob's post").await;
    let comment = app.comment(&alice, post, "Hateful").await;
    app.report(&bob, "comment", comment, "hate_speech").await;
    let ev = record_for(&app, comment).await.unwrap();
    assert_eq!(
        app.scalar(&format!(
            "SELECT (content->'comment'->>'body') || '|' || (content->'context'->'post'->>'caption') FROM evidence_versions WHERE evidence_id = '{ev}'"
        )).await.as_deref(),
        Some("Hateful|Bob's post")
    );

    app.patch(&alice, &format!("/posts/{post}/comments/{comment}"), json!({ "body": "Toned down" })).await.ok();
    assert_eq!(versions(&app, &ev).await, "reported,edited");

    app.delete(&bob, &format!("/posts/{post}")).await.ok();
    assert_eq!(app.scalar(&format!("SELECT deletion_trigger FROM evidence_records WHERE id = '{ev}'")).await.as_deref(), Some("user_deletion"));
    assert_eq!(record_for(&app, post).await, None, "the unreported post itself isn't captured");
}

#[sqlx::test(migrations = "./migrations")]
async fn profile_timeline_through_account_deletion(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (carol, bob, admin) = (app.register("carol").await, app.register("bob").await, app.admin().await);

    app.upload_avatar(&carol, [120, 40, 160]).await;
    let old_avatar = app.scalar(&format!("SELECT avatar_url FROM users WHERE id = '{}'", carol.id)).await.unwrap();
    let report = app.report(&bob, "user", carol.id, "harassment").await;
    let ev = record_for(&app, carol.id).await.unwrap();
    wait_copied(&app, &ev, 1).await;

    app.patch(&carol, "/users/me", json!({ "bio": "New bio" })).await.ok();
    assert_eq!(versions(&app, &ev).await, "reported,edited");
    app.upload_avatar(&carol, [240, 150, 200]).await;
    wait_copied(&app, &ev, 2).await;
    assert_eq!(versions(&app, &ev).await, "reported,edited,edited");
    assert!(!app.media_file(&old_avatar).exists(), "old avatar deleted once copied");

    app.delete(&carol, "/users/me").await.ok();
    assert_eq!(app.scalar(&format!("SELECT deletion_trigger FROM evidence_records WHERE id = '{ev}'")).await.as_deref(), Some("account_deletion"));

    // Confirming the violation on the deleted account keeps the evidence.
    app.post(&admin, &format!("/admin/reports/{report}/remove"), json!({})).await.ok();
    assert_eq!(app.scalar(&format!("SELECT decision FROM evidence_records WHERE id = '{ev}'")).await.as_deref(), Some("removed"));
}

#[sqlx::test(migrations = "./migrations")]
async fn copy_waits_for_evidence_storage_and_the_sweeper_finishes_it(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let without = app.with_evidence(false).await;
    let (alice, bob, admin) = (without.register("alice").await, without.register("bob").await, without.admin().await);

    let post = without.upload(&alice, "Storage down").await;
    let report = without.report(&bob, "post", post, "csam").await;
    let ev = record_for(&without, post).await.unwrap();
    // The CSAM key rotation moves the file; the pending copy follows it.
    eventually("rotation", || async {
        without.scalar(&format!("SELECT source_key FROM evidence_files WHERE evidence_id = '{ev}'")).await
            == without.scalar(&format!("SELECT full_key FROM media_assets WHERE post_id = '{post}'")).await
    }).await;
    assert_eq!(copied_files(&without, &ev).await, 0);

    without.post(&admin, &format!("/admin/reports/{report}/remove"), json!({})).await.ok();
    let source = without.scalar(&format!("SELECT source_key FROM evidence_files WHERE evidence_id = '{ev}'")).await.unwrap();
    assert!(without.media_file(&source).exists(), "original kept while the copy is pending");

    // Storage back: the sweeper copies (once the capture is 5 min old) and
    // then deletes the original.
    let with = app.with_evidence(true).await;
    with.exec(&format!("UPDATE evidence_versions SET captured_at = captured_at - INTERVAL '10 minutes' WHERE evidence_id = '{ev}'")).await;
    evidence::sweep(&with.state).await;
    assert_eq!(copied_files(&with, &ev).await, 1);
    assert!(!with.media_file(&source).exists());
}

#[sqlx::test(migrations = "./migrations")]
async fn reports_from_before_capture_are_preserved_at_deletion(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (alice, bob) = (app.register("alice").await, app.register("bob").await);

    let post = app.upload(&alice, "Legacy").await;
    app.exec(&format!(
        "INSERT INTO reports (reporter_id, target_type, target_id, reason) VALUES ('{}', 'post', '{post}', 'violence')",
        bob.id
    )).await;
    app.delete(&alice, &format!("/posts/{post}")).await.ok();
    let ev = record_for(&app, post).await.expect("captured at deletion");
    assert_eq!(versions(&app, &ev).await, "deleted");
    wait_copied(&app, &ev, 1).await;
}
