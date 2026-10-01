//! Removing parts of a profile -- the picture, the description, the display
//! name, or the username by a forced rename (impersonation, a slur as a
//! name) -- instead of a measure against the whole account. It is a
//! decision like a content removal: classified from the catalog, with a
//! strike, a statement the user can object to, and the reports behind it
//! closed. An accepted objection puts the removed parts back (`restore`).
//! The removed texts are kept in the decision record (removed_fields); the
//! removed picture is moved to a key nothing points to and kept until the
//! objection window has passed (retention.rs), except for CSAM, which goes
//! as soon as its evidence copy exists.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use rand::Rng;
use serde::Deserialize;
use serde_json::json;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::alerts::Alert;
use crate::auth::AuthUser;
use crate::errors::AppError;
use crate::evidence;
use crate::handlers::auth::AppState;
use crate::handlers::reports::{copy_object, require_admin, rotated_key};
use crate::moderation::{self, NewDecision, Restriction, Source};
use crate::standing::{self, AuthorityReport};
use crate::utils::{delete_media, DbResultExt};

const FIELDS: [&str; 4] = ["avatar", "bio", "display_name", "username"];
const NOTE_MAX: usize = 1000;

#[derive(Debug, Deserialize)]
pub struct ProfileRemovalRequest {
    /// Any of "avatar", "bio", "display_name", "username".
    pub fields: Vec<String>,
    /// The violation type from the catalog (or "none"); defaults to the
    /// first linked report's reason.
    pub violation: Option<String>,
    /// Required when departing from the reports' reasons, or for "none".
    pub justification: Option<String>,
    /// Internal, like a review note.
    pub note: Option<String>,
    /// Pending reports on this account that the removal answers.
    #[serde(default)]
    pub report_ids: Vec<Uuid>,
}

fn label_de(field: &str) -> &'static str {
    match field {
        "avatar" => "Profilbild",
        "bio" => "Beschreibung",
        "display_name" => "Anzeigename",
        _ => "Benutzername",
    }
}

fn shortened(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        format!("{}…", text.chars().take(max).collect::<String>())
    }
}

/// A free name of the form user_xxxxxxxx for a forced rename.
async fn forced_username(tx: &mut PgConnection) -> Result<String, AppError> {
    const CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    for _ in 0..10 {
        let suffix: String = (0..8).map(|_| CHARS[rand::rng().random_range(0..CHARS.len())] as char).collect();
        let name = format!("user_{suffix}");
        let taken = sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM users WHERE LOWER(username) = $1)")
            .bind(&name)
            .fetch_one(&mut *tx)
            .await
            .db_err("Database error")?;
        if !taken {
            return Ok(name);
        }
    }
    Err(AppError::internal("Could not find a free username"))
}

/// POST /admin/users/:username/profile-removal (admin only)
pub async fn remove_from_profile(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(username): Path<String>,
    Json(input): Json<ProfileRemovalRequest>,
) -> Result<StatusCode, AppError> {
    require_admin(&state.db, &auth).await?;
    let mut fields: Vec<&str> = FIELDS.iter().copied().filter(|f| input.fields.iter().any(|g| g == f)).collect();
    fields.dedup();
    if fields.is_empty() || fields.len() != input.fields.len() {
        return Err(AppError::bad_request("Pick what to remove: avatar, bio, display_name or username"));
    }
    let note = input.note.as_deref().map(str::trim).filter(|n| !n.is_empty());
    if note.is_some_and(|n| n.chars().count() > NOTE_MAX) {
        return Err(AppError::bad_request(format!("Review note must be under {} characters", NOTE_MAX)));
    }

    let mut tx = state.db.begin().await.db_err("Database error")?;
    let (user_id, current_username, display_name, bio, avatar_url) =
        sqlx::query_as::<_, (Uuid, String, Option<String>, Option<String>, Option<String>)>(
            "SELECT id, username, display_name, bio, avatar_url FROM users WHERE LOWER(username) = LOWER($1) FOR UPDATE",
        )
        .bind(&username)
        .fetch_optional(&mut *tx)
        .await
        .db_err("Database error")?
        .ok_or_else(|| AppError::not_found("User not found"))?;
    if user_id == auth.user_id {
        return Err(AppError::bad_request("You can't moderate your own profile"));
    }
    let present = |value: &Option<String>| value.as_deref().is_some_and(|v| !v.trim().is_empty());
    for field in &fields {
        let there = match *field {
            "avatar" => present(&avatar_url),
            "bio" => present(&bio),
            "display_name" => present(&display_name),
            _ => true,
        };
        if !there {
            return Err(AppError::bad_request(format!("The profile has no {}", field.replace('_', " "))));
        }
    }

    // The reports this answers: pending, on this account.
    let mut report_ids = input.report_ids.clone();
    report_ids.sort();
    report_ids.dedup();
    let reports = sqlx::query_as::<_, (Uuid, String)>(
        r#"
        SELECT id, reason::text FROM reports
        WHERE id = ANY($1) AND target_type = 'user' AND target_id = $2 AND status = 'pending'
        ORDER BY created_at
        FOR UPDATE
        "#,
    )
    .bind(&report_ids)
    .bind(user_id)
    .fetch_all(&mut *tx)
    .await
    .db_err("Database error")?;
    if reports.len() != report_ids.len() {
        return Err(AppError::conflict("Only pending reports on this account can be linked"));
    }
    let mut reasons: Vec<&str> = Vec::new();
    for (_, reason) in &reports {
        if !reasons.contains(&reason.as_str()) {
            reasons.push(reason);
        }
    }

    let classification = standing::classify(&reasons, input.violation.as_deref(), input.justification.as_deref())?;
    let classified = classification.violation;
    let decided_reason = classified.map(|v| v.reason).or(reasons.first().copied()).unwrap_or("other").to_string();
    let source = if report_ids.is_empty() {
        Source::OwnInitiative
    } else {
        moderation::source_of_reports(&mut tx, &report_ids).await?
    };

    // The profile as it was, with its picture, before anything changes --
    // for a likely-illegal classification (a reported one was captured
    // when the report came in).
    let mut preserved = evidence::Preserved::default();
    if let Some(v) = classified {
        preserved.extend(
            evidence::preserve_classified(&mut tx, "user", user_id, v.reason, v.authority_report.as_db(), auth.user_id).await?,
        );
    }

    // What goes: kept in the decision for a restore, listed in the statement.
    let old_avatar = avatar_url.as_deref().map(|url| url.strip_prefix("/media/").unwrap_or(url).to_string());
    let keep_avatar = fields.contains(&"avatar") && decided_reason != "csam";
    let kept_avatar_key = old_avatar.as_deref().filter(|_| keep_avatar).map(|key| rotated_key(key, Uuid::new_v4()));
    let new_username = if fields.contains(&"username") { Some(forced_username(&mut tx).await?) } else { None };

    let mut removed = json!({});
    let mut excerpt_parts = Vec::new();
    for field in &fields {
        match *field {
            "bio" => {
                removed["bio"] = json!(bio);
                excerpt_parts.push(format!("Beschreibung: „{}“", shortened(bio.as_deref().unwrap_or(""), 80)));
            }
            "display_name" => {
                removed["display_name"] = json!(display_name);
                excerpt_parts.push(format!("Anzeigename: „{}“", display_name.as_deref().unwrap_or("")));
            }
            "username" => {
                removed["username"] = json!({ "old": current_username, "new": new_username });
                excerpt_parts.push(format!("Benutzername: „{}“", current_username));
            }
            _ => {
                removed["avatar"] = json!(true);
                if let Some(key) = &kept_avatar_key {
                    removed["avatar_key"] = json!(key);
                }
                excerpt_parts.push("Profilbild".to_string());
            }
        }
    }
    let mut detail = format!("Entfernt: {}.", fields.iter().map(|f| label_de(f)).collect::<Vec<_>>().join(", "));
    if let Some(name) = &new_username {
        detail = format!(
            "{} Dein Benutzername lautet jetzt „{}“; in den Einstellungen kannst du einen neuen wählen, der nicht gegen \
             unsere Nutzungsbedingungen verstößt.",
            detail, name
        );
    }

    let recorded = moderation::record_decision(&mut tx, NewDecision {
        decided_by: Some(auth.user_id),
        report_ids: report_ids.clone(),
        source,
        detail: Some(detail),
        classification: Some(classification),
        excerpt: Some(shortened(&excerpt_parts.join(" · "), 200)),
        removed_fields: Some(removed),
        ..NewDecision::base("user", user_id, Restriction::Removed, &decided_reason)
    })
    .await?;
    let mut notices = recorded.notices;

    sqlx::query(
        r#"
        UPDATE users SET
            bio = CASE WHEN $2 THEN NULL ELSE bio END,
            display_name = CASE WHEN $3 THEN NULL ELSE display_name END,
            avatar_url = CASE WHEN $4 THEN NULL ELSE avatar_url END,
            username = COALESCE($5, username),
            -- A forced name can be replaced at once, without the cooldown.
            username_changed_at = CASE WHEN $5 IS NULL THEN username_changed_at END
        WHERE id = $1
        "#,
    )
    .bind(user_id)
    .bind(fields.contains(&"bio"))
    .bind(fields.contains(&"display_name"))
    .bind(fields.contains(&"avatar"))
    .bind(&new_username)
    .execute(&mut *tx)
    .await
    .db_err("Failed to update profile")?;

    notices.extend(moderation::close_reports(&mut tx, &report_ids, "actioned", "removed", Some(auth.user_id), note).await?);
    evidence::decide(&mut tx, "user", user_id, auth.user_id, note).await?;
    if classified.is_some_and(|v| v.authority_report == AuthorityReport::Required) {
        let evidence_id = sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM evidence_records WHERE target_type = 'user' AND target_id = $1 AND purged_at IS NULL ORDER BY created_at DESC LIMIT 1",
        )
        .bind(user_id)
        .fetch_optional(&mut *tx)
        .await
        .db_err("Database error")?;
        notices.alert(Alert::AuthorityReportRequired { target_type: "user".into(), target_id: user_id, evidence_id });
    }
    tx.commit().await.db_err("Database error")?;
    notices.send(&state).await;

    // The evidence copy reads the picture first; then it moves out of
    // reach, or goes.
    evidence::finish(&state, preserved, Vec::new()).await;
    if let Some(old) = old_avatar.filter(|_| fields.contains(&"avatar")) {
        match (&kept_avatar_key, recorded.decision_id) {
            (Some(kept), Some(decision_id)) => keep_removed_avatar(&state, decision_id, &old, kept).await,
            _ => evidence::release_media(&state, &old).await,
        }
    }

    tracing::info!("Profile of {} moderated by admin {}: {:?}", user_id, auth.user_id, fields);
    Ok(StatusCode::NO_CONTENT)
}

/// Moves a removed profile picture to its kept key, out of reach of any
/// link handed out before. If the copy fails, the decision keeps the old
/// key instead and only the CDN copy is purged.
async fn keep_removed_avatar(state: &AppState, decision_id: Uuid, old: &str, kept: &str) {
    match copy_object(state, old, kept).await {
        Ok(()) => {
            if let Err(e) = evidence::follow_moved_source(state, old, kept).await {
                tracing::error!("Evidence copy of removed avatar {} not repointed: {}", old, e.message);
            }
            delete_media(state, old).await;
        }
        Err(e) => {
            tracing::error!("Moving removed avatar {} failed, keeping it in place: {}", old, e.message);
            if let Err(e) = state.cdn.purge(&state.storage.public_url(old)).await {
                tracing::error!("Failed to purge avatar {} from CDN: {}", old, e.message);
            }
            let _ = sqlx::query(
                "UPDATE moderation_decisions SET removed_fields = jsonb_set(removed_fields, '{avatar_key}', to_jsonb($2::text)) WHERE id = $1",
            )
            .bind(decision_id)
            .bind(old)
            .execute(&state.db)
            .await;
        }
    }
}

/// After an accepted objection to a removal from a profile: puts back what
/// was removed where nothing new has taken its place -- a description
/// written since stays, and the old username only comes back while the
/// forced one is still in use and the old one is free. Returns a kept
/// picture that couldn't go back (the user has a new one), for the caller
/// to delete after commit.
pub async fn restore(tx: &mut PgConnection, decision_id: Uuid, user_id: Uuid) -> Result<Option<String>, AppError> {
    let Some((fields, purged)) = sqlx::query_as::<_, (Option<serde_json::Value>, bool)>(
        "SELECT removed_fields, content_purged_at IS NOT NULL FROM moderation_decisions WHERE id = $1",
    )
    .bind(decision_id)
    .fetch_optional(&mut *tx)
    .await
    .db_err("Database error")?
    else {
        return Ok(None);
    };
    let Some(fields) = fields else {
        return Ok(None);
    };

    for (field, column) in [("bio", "bio"), ("display_name", "display_name")] {
        if let Some(value) = fields.get(field).and_then(|v| v.as_str()) {
            sqlx::query(&format!(
                "UPDATE users SET {column} = $2 WHERE id = $1 AND ({column} IS NULL OR {column} = '')"
            ))
            .bind(user_id)
            .bind(value)
            .execute(&mut *tx)
            .await
            .db_err("Database error")?;
        }
    }
    if let (Some(old), Some(new)) = (
        fields.pointer("/username/old").and_then(|v| v.as_str()),
        fields.pointer("/username/new").and_then(|v| v.as_str()),
    ) {
        sqlx::query(
            r#"
            UPDATE users SET username = $2
            WHERE id = $1 AND username = $3
              AND NOT EXISTS (SELECT 1 FROM users o WHERE LOWER(o.username) = LOWER($2) AND o.id != $1)
            "#,
        )
        .bind(user_id)
        .bind(old)
        .bind(new)
        .execute(&mut *tx)
        .await
        .db_err("Database error")?;
    }
    if let Some(key) = fields.get("avatar_key").and_then(|v| v.as_str()).filter(|_| !purged) {
        let restored = sqlx::query("UPDATE users SET avatar_url = $2 WHERE id = $1 AND avatar_url IS NULL")
            .bind(user_id)
            .bind(key)
            .execute(&mut *tx)
            .await
            .db_err("Database error")?
            .rows_affected();
        if restored == 0 {
            return Ok(Some(key.to_string()));
        }
    }
    Ok(None)
}
