//! Rights claims (handlers/rights.rs): the public form, the claimant's
//! status link, triage, acceptance with a statement for the uploader,
//! restoration after an objection, declines and retention.

use axum::http::StatusCode;
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use super::support::*;
use crate::handlers::rights::delete_expired;

fn claim(post: Uuid) -> Value {
    json!({
        "claim_type": "copyright", "claimant_name": "Erika Fotografin", "claimant_email": "erika@example.test",
        "claimant_organization": null, "represented_party": null,
        "content_url": format!("https://www.klarsocial.eu/posts/{post}"),
        "work_description": "My photo 'Sunset over Elbe'", "ownership_basis": "I took it on 3 May 2026",
        "original_url": null, "good_faith": true, "website": ""
    })
}

fn with(mut body: Value, key: &str, value: Value) -> Value {
    body[key] = value;
    body
}

#[sqlx::test(migrations = "./migrations")]
async fn the_public_form_validates_and_ignores_bots(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let alice = app.register("alice").await;
    let post = app.upload(&alice, "Sunset").await;

    for (body, status, why) in [
        (with(claim(post), "good_faith", json!(false)), StatusCode::BAD_REQUEST, "good faith"),
        (with(claim(post), "content_url", json!("https://www.klarsocial.eu/users/x")), StatusCode::BAD_REQUEST, "not a post"),
        (claim(Uuid::new_v4()), StatusCode::NOT_FOUND, "unknown post"),
        (with(claim(post), "claimant_email", json!("nope")), StatusCode::BAD_REQUEST, "bad email"),
    ] {
        assert_eq!(app.anon_post("/rights-claims", body).await.status, status, "{why}");
    }

    let res = app.anon_post("/rights-claims", with(claim(post), "website", json!("http://spam.example"))).await;
    assert_eq!(res.status, StatusCode::CREATED, "the honeypot looks accepted");
    assert_eq!(app.count("SELECT 1 FROM rights_claims").await, 0, "but nothing is stored");
}

#[sqlx::test(migrations = "./migrations")]
async fn claim_through_evidence_request_acceptance_and_restoration(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (uploader, admin) = (app.register("uploader").await, app.admin().await);
    let post = app.upload(&uploader, "Look at this sunset").await;
    let key_before = app.scalar(&format!("SELECT full_key FROM media_assets WHERE post_id = '{post}'")).await;

    let res = app.anon_post("/rights-claims", claim(post)).await.ok().json();
    let id = res["id"].as_str().unwrap().to_string();
    let token = res["token"].as_str().unwrap().to_string();
    assert_eq!(
        app.scalar(&format!("SELECT status_token_hash <> '{token}' AND length(status_token_hash) = 64 FROM rights_claims WHERE id = '{id}'")).await.as_deref(),
        Some("true"),
        "only a hash of the token is stored"
    );

    let status = |token: String| {
        let (app, id) = (&app, id.clone());
        async move { app.anon_post(&format!("/rights-claims/{id}/status"), json!({ "token": token })).await }
    };
    let st = status(token.clone()).await.ok().json();
    assert_eq!(st["status"], "submitted");
    assert!(st.get("claimant_email").is_none());
    assert_eq!(status("x".repeat(64)).await.status, StatusCode::NOT_FOUND);
    let respond = format!("/rights-claims/{id}/respond");
    assert_eq!(app.anon_post(&respond, json!({ "token": token, "response": "hi" })).await.status, StatusCode::CONFLICT);

    assert_eq!(app.get(&uploader, "/admin/rights-claims").await.status, StatusCode::FORBIDDEN);
    let row = app.get(&admin, "/admin/rights-claims").await.ok().json()[0].clone();
    assert_eq!(row["target_exists"], true);
    assert_eq!(row["target_username"], "uploader");

    app.post(&admin, &format!("/admin/rights-claims/{id}/triage"), json!({})).await.ok();
    assert_eq!(app.post(&admin, &format!("/admin/rights-claims/{id}/triage"), json!({})).await.status, StatusCode::CONFLICT);
    app.post(&admin, &format!("/admin/rights-claims/{id}/request-evidence"), json!({ "message": "Please send the RAW file name." })).await.ok();
    assert_eq!(status(token.clone()).await.ok().json()["evidence_request"], "Please send the RAW file name.");
    let st = app.anon_post(&respond, json!({ "token": token, "response": "IMG_4711.CR3" })).await.ok().json();
    assert_eq!(st["status"], "triaged");

    // Accepting hides the post, moves its files and tells the uploader —
    // about the work, not about who claimed it.
    app.post(&admin, &format!("/admin/rights-claims/{id}/accept"), json!({})).await.ok();
    assert_eq!(app.scalar(&format!("SELECT moderation_status FROM posts WHERE id = '{post}'")).await.as_deref(), Some("hidden"));
    eventually("key rotation", || async {
        app.scalar(&format!("SELECT full_key FROM media_assets WHERE post_id = '{post}'")).await != key_before
    }).await;
    let ds = app.get(&uploader, "/moderation/decisions").await.ok().json();
    let d = &ds[0];
    assert_eq!(d["reason"], "copyright");
    assert_eq!(d["restriction"], "hidden");
    assert_eq!(d["automated"], false);
    assert_eq!(d["ground_type"], "illegal");
    assert!(d["explanation"].as_str().unwrap().contains("Sunset over Elbe"));
    assert!(!ds.to_string().contains("Erika") && !ds.to_string().contains("erika@"));
    assert_eq!(app.post(&admin, &format!("/admin/rights-claims/{id}/accept"), json!({})).await.status, StatusCode::NOT_FOUND);

    // A successful objection restores the post and closes the claim.
    let decision = d["id"].as_str().unwrap();
    app.post(&uploader, &format!("/moderation/decisions/{decision}/objection"), json!({ "text": "I licensed this photo, see invoice 12." })).await.ok();
    app.post(&admin, &format!("/admin/moderation/decisions/{decision}/objection"), json!({ "accept": true, "response": "The licence checks out." })).await.ok();
    assert_eq!(app.scalar(&format!("SELECT moderation_status FROM posts WHERE id = '{post}'")).await.as_deref(), Some("visible"));
    assert_eq!(app.scalar(&format!("SELECT status FROM rights_claims WHERE id = '{id}'")).await.as_deref(), Some("restored"));
    assert_eq!(
        app.scalar(&format!("SELECT string_agg(action, ',' ORDER BY created_at, id) FROM rights_claim_events WHERE claim_id = '{id}'")).await.as_deref(),
        Some("submitted,triaged,evidence_requested,claimant_responded,accepted,restored")
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn declines_deleted_posts_export_and_retention(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (uploader, claimant, admin) = (app.register("uploader").await, app.register("claimant").await, app.admin().await);

    // Signed in: linked to the account and in its export.
    let post = app.upload(&uploader, "Another photo").await;
    let res = app.post(&claimant, "/rights-claims", with(claim(post), "claim_type", json!("trademark"))).await.ok().json();
    let (id, token) = (res["id"].as_str().unwrap().to_string(), res["token"].as_str().unwrap().to_string());
    assert_eq!(app.scalar(&format!("SELECT claimant_user_id FROM rights_claims WHERE id = '{id}'")).await, Some(claimant.id.to_string()));

    assert_eq!(app.post(&admin, &format!("/admin/rights-claims/{id}/decline"), json!({ "message": "" })).await.status, StatusCode::BAD_REQUEST);
    app.post(&admin, &format!("/admin/rights-claims/{id}/decline"), json!({ "message": "The logo is only visible incidentally." })).await.ok();
    let st = app.anon_post(&format!("/rights-claims/{id}/status"), json!({ "token": token })).await.ok().json();
    assert_eq!(st["status"], "declined");
    assert_eq!(st["decision_reason"], "The logo is only visible incidentally.");
    assert_eq!(app.scalar(&format!("SELECT moderation_status FROM posts WHERE id = '{post}'")).await.as_deref(), Some("visible"));
    assert_eq!(app.export(&claimant).await["moderation"]["rights_claims_filed"].as_array().unwrap().len(), 1);

    // A claim on a post deleted meanwhile can't be accepted.
    let gone = app.upload(&uploader, "Soon deleted").await;
    let gone_id = app.anon_post("/rights-claims", claim(gone)).await.ok().json()["id"].as_str().unwrap().to_string();
    app.delete(&uploader, &format!("/posts/{gone}")).await.ok();
    assert_eq!(app.post(&admin, &format!("/admin/rights-claims/{gone_id}/accept"), json!({})).await.status, StatusCode::CONFLICT);

    // Decided claims go three years after the decision; open ones stay.
    app.exec(&format!("UPDATE rights_claims SET decided_at = NOW() - INTERVAL '1096 days' WHERE id = '{id}'")).await;
    assert_eq!(delete_expired(&app.state.db).await.unwrap(), 1);
    assert_eq!(app.count(&format!("SELECT 1 FROM rights_claim_events WHERE claim_id = '{id}'")).await, 0);
    assert_eq!(app.count(&format!("SELECT 1 FROM rights_claims WHERE id = '{gone_id}'")).await, 1);
}
