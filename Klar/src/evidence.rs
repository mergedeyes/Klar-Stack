//! Evidence preservation for likely-illegal content.
//!
//! Every path that deletes reported content (admin removal, a user deleting
//! a post or comment, account deletion) calls `preserve` inside its
//! transaction, before the delete. Anything in scope that has a *pending*
//! report for a likely-illegal reason is snapshotted into evidence_records:
//! the content, its author, its context and every report on it. The
//! caller then commits and hands its media keys to `finish`, which copies
//! the preserved files into the evidence storage zone and only then deletes
//! the originals.
//!
//! A file whose copy fails (or the evidence zone not being configured)
//! never loses evidence and never blocks a deletion: the original stays at
//! its key, its CDN cache is purged, and the sweeper retries the copy.
//!
//! Records are decided when the last likely-illegal report on their target
//! is resolved (`decide`): kept for the retention period if any report was
//! actioned, purged straight away if they were all dismissed. A legal hold
//! stops the purge. See migrations/20260930010000_evidence.sql for the
//! tables, and handlers/evidence.rs for the admin endpoints.
//!
//! The data export (Art. 15) deliberately doesn't include evidence records:
//! disclosing them to the reported person could compromise an
//! investigation (Art. 23 GDPR, §33 BDSG) -- pending legal review.

use std::collections::HashSet;
use std::time::Duration;

use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::errors::AppError;
use crate::handlers::auth::AppState;
use crate::utils::{delete_media, DbResultExt};

/// Report reasons that may point to a crime, so the content is preserved
/// when deleted. Spam, impersonation and "other" are deleted outright.
/// Kept as one list so a lawyer's answer is a one-line change.
pub const LIKELY_ILLEGAL_REASONS: &[&str] = &[
    "csam", "violence", "hate_speech", "harassment", "sexual_content", "self_harm",
];

/// How long evidence is kept after a "removed" decision, unless a legal
/// hold is set. Six months by default, pending legal review; override with
/// EVIDENCE_RETENTION_DAYS.
fn retention_days() -> i32 {
    std::env::var("EVIDENCE_RETENTION_DAYS")
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .filter(|days| *days > 0)
        .unwrap_or(183)
}

fn likely_illegal() -> Vec<String> {
    LIKELY_ILLEGAL_REASONS.iter().map(|r| r.to_string()).collect()
}

#[derive(Clone, Copy)]
pub enum Trigger {
    ModerationRemoval,
    /// The author deleting their post or comment, or a post owner deleting
    /// a comment on their post.
    UserDeletion,
    AccountDeletion,
}

impl Trigger {
    fn as_str(self) -> &'static str {
        match self {
            Trigger::ModerationRemoval => "moderation_removal",
            Trigger::UserDeletion => "user_deletion",
            Trigger::AccountDeletion => "account_deletion",
        }
    }
}

/// What the caller is about to delete. Comments cascade with their post
/// and with their parent comment, so the reported ones among those are
/// found here rather than asked of every caller.
#[derive(Default)]
pub struct Scope {
    pub posts: Vec<Uuid>,
    /// Roots of the comment subtrees being deleted.
    pub comments: Vec<Uuid>,
    pub user: Option<Uuid>,
}

struct PendingFile {
    id: Uuid,
    source_key: String,
    storage_key: String,
}

/// Returned by `preserve`, consumed by `finish` after the commit.
#[must_use]
pub struct Preserved {
    files: Vec<PendingFile>,
}

// SQL fragments shared by the snapshot queries, given the column that
// holds the user's or target's id.
fn author_json(user_id_col: &str) -> String {
    format!(
        "(SELECT jsonb_build_object('id', u.id, 'username', u.username, 'display_name', u.display_name, \
         'email', u.email, 'created_at', u.created_at) FROM users u WHERE u.id = {user_id_col})"
    )
}

fn reports_json(target_type: &str, id_col: &str) -> String {
    format!(
        "(SELECT COALESCE(jsonb_agg(jsonb_build_object('id', r.id, 'reason', r.reason, 'details', r.details, \
         'status', r.status, 'created_at', r.created_at, 'reporter_id', r.reporter_id) ORDER BY r.created_at), '[]'::jsonb) \
         FROM reports r WHERE r.target_type = '{target_type}' AND r.target_id = {id_col})"
    )
}

fn reasons_sql(target_type: &str, id_col: &str) -> String {
    format!(
        "(SELECT COALESCE(array_agg(DISTINCT r.reason::text), '{{}}') FROM reports r \
         WHERE r.target_type = '{target_type}' AND r.target_id = {id_col})"
    )
}

/// Snapshots everything in `scope` that has a pending likely-illegal
/// report. Runs inside the caller's transaction, before its delete, so
/// the snapshot and the deletion commit or roll back together.
pub async fn preserve(
    tx: &mut PgConnection,
    scope: &Scope,
    trigger: Trigger,
    actor_id: Option<Uuid>,
) -> Result<Preserved, AppError> {
    let reasons = likely_illegal();
    let mut preserved = Preserved { files: Vec::new() };

    let posts = sqlx::query_scalar::<_, Uuid>(
        r#"
        SELECT p.id FROM posts p
        WHERE p.id = ANY($1) AND EXISTS (
            SELECT 1 FROM reports r
            WHERE r.target_type = 'post' AND r.target_id = p.id
              AND r.status = 'pending' AND r.reason::text = ANY($2)
        )
        "#,
    )
    .bind(&scope.posts)
    .bind(&reasons)
    .fetch_all(&mut *tx)
    .await
    .db_err_ctx("Evidence: finding reported posts failed", "Database error")?;

    let comments = sqlx::query_scalar::<_, Uuid>(
        r#"
        WITH RECURSIVE doomed AS (
            SELECT id FROM comments WHERE id = ANY($1) OR post_id = ANY($2)
            UNION
            SELECT c.id FROM comments c JOIN doomed d ON c.parent_comment_id = d.id
        )
        SELECT d.id FROM doomed d
        WHERE EXISTS (
            SELECT 1 FROM reports r
            WHERE r.target_type = 'comment' AND r.target_id = d.id
              AND r.status = 'pending' AND r.reason::text = ANY($3)
        )
        "#,
    )
    .bind(&scope.comments)
    .bind(&scope.posts)
    .bind(&reasons)
    .fetch_all(&mut *tx)
    .await
    .db_err_ctx("Evidence: finding reported comments failed", "Database error")?;

    let user = match scope.user {
        Some(user_id) => sqlx::query_scalar::<_, bool>(
            r#"
            SELECT EXISTS (
                SELECT 1 FROM reports r
                WHERE r.target_type = 'user' AND r.target_id = $1
                  AND r.status = 'pending' AND r.reason::text = ANY($2)
            )
            "#,
        )
        .bind(user_id)
        .bind(&reasons)
        .fetch_one(&mut *tx)
        .await
        .db_err_ctx("Evidence: checking user reports failed", "Database error")?
        .then_some(user_id),
        None => None,
    };

    for post_id in posts {
        let evidence_id = insert_record(tx, &post_snapshot_sql(), post_id, trigger).await?;
        // FOR UPDATE: a concurrent media key rotation (reports.rs, on a
        // CSAM hide) then waits for this transaction and finds the rows
        // gone, instead of moving the files away underneath the copy.
        let media = sqlx::query_as::<_, (String, i32)>(
            "SELECT full_key, sort_order FROM media_assets WHERE post_id = $1 ORDER BY sort_order FOR UPDATE",
        )
        .bind(post_id)
        .fetch_all(&mut *tx)
        .await
        .db_err_ctx("Evidence: reading post media failed", "Database error")?;
        for (key, sort_order) in media {
            add_file(tx, &mut preserved, evidence_id, "post_media", sort_order, key).await?;
        }
        log_created(tx, evidence_id, actor_id, trigger).await?;
    }

    for comment_id in comments {
        let evidence_id = insert_record(tx, &comment_snapshot_sql(), comment_id, trigger).await?;
        log_created(tx, evidence_id, actor_id, trigger).await?;
    }

    if let Some(user_id) = user {
        let evidence_id = insert_record(tx, &user_snapshot_sql(), user_id, trigger).await?;
        let avatar = sqlx::query_scalar::<_, Option<String>>("SELECT avatar_url FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_one(&mut *tx)
            .await
            .db_err_ctx("Evidence: reading avatar failed", "Database error")?;
        if let Some(avatar) = avatar {
            // Older rows carry a "/media/" prefix (see delete_account).
            let key = avatar.strip_prefix("/media/").unwrap_or(&avatar).to_string();
            add_file(tx, &mut preserved, evidence_id, "avatar", 0, key).await?;
        }
        log_created(tx, evidence_id, actor_id, trigger).await?;
    }

    Ok(preserved)
}

fn post_snapshot_sql() -> String {
    format!(
        r#"
        SELECT 'post'::report_target_type, p.id, $2, {reasons},
            jsonb_build_object(
                'post', jsonb_build_object('id', p.id, 'caption', p.caption, 'created_at', p.created_at,
                    'edited_at', p.edited_at, 'moderation_status', p.moderation_status),
                'author', {author},
                'reports', {reports})
        FROM posts p WHERE p.id = $1
        "#,
        reasons = reasons_sql("post", "p.id"),
        author = author_json("p.user_id"),
        reports = reports_json("post", "p.id"),
    )
}

fn comment_snapshot_sql() -> String {
    format!(
        r#"
        SELECT 'comment'::report_target_type, c.id, $2, {reasons},
            jsonb_build_object(
                'comment', jsonb_build_object('id', c.id, 'body', c.body, 'created_at', c.created_at,
                    'edited_at', c.edited_at, 'moderation_status', c.moderation_status),
                'author', {author},
                'context', jsonb_build_object(
                    'post', (SELECT jsonb_build_object('id', p.id, 'caption', p.caption,
                        'author_id', p.user_id, 'created_at', p.created_at) FROM posts p WHERE p.id = c.post_id),
                    'parent_comment', (SELECT jsonb_build_object('id', pc.id, 'body', pc.body,
                        'author_id', pc.user_id, 'created_at', pc.created_at) FROM comments pc WHERE pc.id = c.parent_comment_id)),
                'reports', {reports})
        FROM comments c WHERE c.id = $1
        "#,
        reasons = reasons_sql("comment", "c.id"),
        author = author_json("c.user_id"),
        reports = reports_json("comment", "c.id"),
    )
}

fn user_snapshot_sql() -> String {
    format!(
        r#"
        SELECT 'user'::report_target_type, u0.id, $2, {reasons},
            jsonb_build_object(
                'profile', jsonb_build_object('id', u0.id, 'username', u0.username,
                    'display_name', u0.display_name, 'bio', u0.bio, 'created_at', u0.created_at),
                'author', {author},
                'reports', {reports})
        FROM users u0 WHERE u0.id = $1
        "#,
        reasons = reasons_sql("user", "u0.id"),
        author = author_json("u0.id"),
        reports = reports_json("user", "u0.id"),
    )
}

async fn insert_record(
    tx: &mut PgConnection,
    snapshot_sql: &str,
    target_id: Uuid,
    trigger: Trigger,
) -> Result<Uuid, AppError> {
    let sql = format!(
        "INSERT INTO evidence_records (target_type, target_id, trigger, reasons, content) {snapshot_sql} RETURNING id"
    );
    sqlx::query_scalar::<_, Uuid>(&sql)
        .bind(target_id)
        .bind(trigger.as_str())
        .fetch_one(&mut *tx)
        .await
        .db_err_ctx("Evidence: saving snapshot failed", "Database error")
}

async fn add_file(
    tx: &mut PgConnection,
    preserved: &mut Preserved,
    evidence_id: Uuid,
    kind: &str,
    sort_order: i32,
    source_key: String,
) -> Result<(), AppError> {
    let id = Uuid::new_v4();
    let ext = source_key.rsplit_once('.').map(|(_, ext)| ext.to_ascii_lowercase()).unwrap_or_default();
    let storage_key = if ext.is_empty() {
        format!("{evidence_id}/{id}")
    } else {
        format!("{evidence_id}/{id}.{ext}")
    };

    sqlx::query(
        r#"
        INSERT INTO evidence_files (id, evidence_id, kind, sort_order, source_key, storage_key, content_type)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        "#,
    )
    .bind(id)
    .bind(evidence_id)
    .bind(kind)
    .bind(sort_order)
    .bind(&source_key)
    .bind(&storage_key)
    .bind(content_type_for(&storage_key))
    .execute(&mut *tx)
    .await
    .db_err_ctx("Evidence: saving file entry failed", "Database error")?;

    preserved.files.push(PendingFile { id, source_key, storage_key });
    Ok(())
}

async fn log_created(
    tx: &mut PgConnection,
    evidence_id: Uuid,
    actor_id: Option<Uuid>,
    trigger: Trigger,
) -> Result<(), AppError> {
    log_event(tx, evidence_id, actor_id, "created", Some(trigger.as_str()), None).await?;
    tracing::info!("Evidence {} preserved ({})", evidence_id, trigger.as_str());
    Ok(())
}

pub async fn log_event(
    conn: &mut PgConnection,
    evidence_id: Uuid,
    actor_id: Option<Uuid>,
    action: &str,
    reason: Option<&str>,
    details: Option<serde_json::Value>,
) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO evidence_events (evidence_id, actor_id, action, reason, details) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(evidence_id)
    .bind(actor_id)
    .bind(action)
    .bind(reason)
    .bind(details)
    .execute(&mut *conn)
    .await
    .db_err_ctx("Evidence: writing audit event failed", "Database error")?;
    Ok(())
}

pub fn content_type_for(key: &str) -> &'static str {
    match key.rsplit_once('.').map(|(_, ext)| ext) {
        Some("webp") => "image/webp",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        _ => "application/octet-stream",
    }
}

/// After the caller's commit: copies the preserved files into the evidence
/// zone, then deletes `media_keys` (every file of the deleted content) --
/// except originals whose copy failed, which stay for the sweeper and only
/// get their CDN cache purged.
pub async fn finish(state: &AppState, preserved: Preserved, media_keys: impl IntoIterator<Item = String>) {
    let mut kept = HashSet::new();
    for file in &preserved.files {
        if let Err(e) = copy_file(state, file.id, &file.source_key, &file.storage_key).await {
            tracing::error!(
                "Evidence file {} not copied yet, keeping {} for the sweeper: {}",
                file.id, file.source_key, e.message
            );
            kept.insert(file.source_key.clone());
        }
    }

    for key in media_keys {
        if kept.contains(&key) {
            if let Err(e) = state.cdn.purge(&state.storage.public_url(&key)).await {
                tracing::error!("Failed to purge media {} from CDN: {}", key, e.message);
            }
        } else {
            delete_media(state, &key).await;
        }
    }
}

async fn copy_file(state: &AppState, file_id: Uuid, source_key: &str, storage_key: &str) -> Result<(), AppError> {
    if !state.evidence.is_configured() {
        return Err(AppError::internal("Evidence storage is not configured"));
    }
    let data = state.storage.get(source_key).await?;
    let sha256 = hex::encode(Sha256::digest(&data));
    state.evidence.save(storage_key, &data).await?;

    sqlx::query(
        "UPDATE evidence_files SET copied_at = NOW(), size_bytes = $2, sha256 = $3 WHERE id = $1 AND copied_at IS NULL",
    )
    .bind(file_id)
    .bind(data.len() as i64)
    .bind(&sha256)
    .execute(&state.db)
    .await
    .db_err_ctx("Evidence: marking file copied failed", "Database error")?;
    Ok(())
}

/// Called whenever a report is resolved, inside the resolving
/// transaction. Once no likely-illegal report on the target is pending any
/// more, its undecided evidence gets its decision: 'removed' (kept for the
/// retention period) if any report on it was actioned, else 'dismissed'
/// (purged by the next sweep).
pub async fn decide(
    tx: &mut PgConnection,
    target_type: &str,
    target_id: Uuid,
    admin_id: Uuid,
    note: Option<&str>,
) -> Result<(), AppError> {
    let rows = sqlx::query_as::<_, (Uuid, String)>(
        r#"
        WITH outcome AS (
            SELECT CASE WHEN EXISTS (
                SELECT 1 FROM reports WHERE target_type = $1::report_target_type AND target_id = $2 AND status = 'actioned'
            ) THEN 'removed' ELSE 'dismissed' END AS decision
        )
        UPDATE evidence_records e
        SET decision = o.decision, decided_at = NOW(), decided_by = $3, decision_note = $4,
            retain_until = CASE WHEN o.decision = 'removed' THEN NOW() + make_interval(days => $5) ELSE NOW() END
        FROM outcome o
        WHERE e.target_type = $1::report_target_type AND e.target_id = $2
          AND e.decided_at IS NULL AND e.purged_at IS NULL
          AND NOT EXISTS (
              SELECT 1 FROM reports r
              WHERE r.target_type = $1::report_target_type AND r.target_id = $2
                AND r.status = 'pending' AND r.reason::text = ANY($6)
          )
        RETURNING e.id, e.decision
        "#,
    )
    .bind(target_type)
    .bind(target_id)
    .bind(admin_id)
    .bind(note)
    .bind(retention_days())
    .bind(likely_illegal())
    .fetch_all(&mut *tx)
    .await
    .db_err_ctx("Evidence: recording decision failed", "Database error")?;

    for (evidence_id, decision) in rows {
        log_event(tx, evidence_id, Some(admin_id), "decided", note, Some(json!({ "decision": decision }))).await?;
    }
    Ok(())
}

/// Runs the sweep once an hour on every replica. The row locks in each
/// step (SKIP LOCKED) keep replicas from working on the same item.
pub fn spawn_sweeper(state: AppState) {
    tokio::spawn(async move {
        loop {
            sweep(&state).await;
            tokio::time::sleep(Duration::from_secs(60 * 60)).await;
        }
    });
}

async fn sweep(state: &AppState) {
    retry_pending_copies(state).await;
    purge_due(state).await;
}

/// Copies files whose copy failed at deletion time, then deletes their
/// originals. Skips anything younger than five minutes, which `finish`
/// is most likely still working on.
async fn retry_pending_copies(state: &AppState) {
    if !state.evidence.is_configured() {
        return;
    }
    let pending = match sqlx::query_as::<_, (Uuid, String, String)>(
        r#"
        SELECT f.id, f.source_key, f.storage_key
        FROM evidence_files f JOIN evidence_records e ON e.id = f.evidence_id
        WHERE f.copied_at IS NULL AND e.purged_at IS NULL
          AND e.created_at < NOW() - INTERVAL '5 minutes'
        LIMIT 100
        "#,
    )
    .fetch_all(&state.db)
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            tracing::error!("Evidence sweep: listing pending files failed: {}", e);
            return;
        }
    };

    for (file_id, source_key, storage_key) in pending {
        match copy_file(state, file_id, &source_key, &storage_key).await {
            Ok(()) => {
                delete_media(state, &source_key).await;
                tracing::info!("Evidence sweep: copied file {}", file_id);
            }
            Err(e) => tracing::error!("Evidence sweep: file {} still not copied: {}", file_id, e.message),
        }
    }
}

/// Purges records whose retention has passed and that aren't on hold: the
/// files are deleted, content becomes NULL, and a 'purged' event is logged.
async fn purge_due(state: &AppState) {
    let due = match sqlx::query_scalar::<_, Uuid>(
        r#"
        SELECT id FROM evidence_records
        WHERE purged_at IS NULL AND NOT legal_hold AND retain_until <= NOW()
        ORDER BY retain_until
        LIMIT 50
        "#,
    )
    .fetch_all(&state.db)
    .await
    {
        Ok(ids) => ids,
        Err(e) => {
            tracing::error!("Evidence sweep: listing due records failed: {}", e);
            return;
        }
    };

    for evidence_id in due {
        if let Err(e) = purge(state, evidence_id).await {
            tracing::error!("Evidence sweep: purging {} failed, retrying next sweep: {}", evidence_id, e.message);
        }
    }
}

async fn purge(state: &AppState, evidence_id: Uuid) -> Result<(), AppError> {
    let mut tx = state.db.begin().await.db_err("Database error")?;

    // Re-checked under the lock: a hold may have been set since the scan.
    let Some(decision) = sqlx::query_scalar::<_, Option<String>>(
        r#"
        SELECT decision FROM evidence_records
        WHERE id = $1 AND purged_at IS NULL AND NOT legal_hold AND retain_until <= NOW()
        FOR UPDATE SKIP LOCKED
        "#,
    )
    .bind(evidence_id)
    .fetch_optional(&mut *tx)
    .await
    .db_err("Database error")?
    else {
        return Ok(());
    };

    let files = sqlx::query_as::<_, (String, String, bool)>(
        "SELECT source_key, storage_key, copied_at IS NOT NULL FROM evidence_files WHERE evidence_id = $1",
    )
    .bind(evidence_id)
    .fetch_all(&mut *tx)
    .await
    .db_err("Database error")?;

    // An error returns before the commit, so the record stays and the next
    // sweep tries again. Deleting an already-deleted file succeeds, so the
    // retry is safe.
    for (source_key, storage_key, copied) in files {
        if copied {
            state.evidence.delete(&storage_key).await?;
        } else {
            // Never copied: the only copy is still the original.
            state.storage.delete(&source_key).await?;
            if let Err(e) = state.cdn.purge(&state.storage.public_url(&source_key)).await {
                tracing::error!("Failed to purge media {} from CDN: {}", source_key, e.message);
            }
        }
    }

    sqlx::query("DELETE FROM evidence_files WHERE evidence_id = $1")
        .bind(evidence_id)
        .execute(&mut *tx)
        .await
        .db_err("Database error")?;
    sqlx::query("UPDATE evidence_records SET content = NULL, purged_at = NOW() WHERE id = $1")
        .bind(evidence_id)
        .execute(&mut *tx)
        .await
        .db_err("Database error")?;

    let reason = if decision.as_deref() == Some("dismissed") {
        "reports dismissed"
    } else {
        "retention period ended"
    };
    log_event(&mut tx, evidence_id, None, "purged", Some(reason), None).await?;

    tx.commit().await.db_err("Database error")?;
    tracing::info!("Evidence {} purged ({})", evidence_id, reason);
    Ok(())
}
