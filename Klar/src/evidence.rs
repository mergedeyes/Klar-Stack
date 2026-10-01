//! Evidence preservation for likely-illegal content.
//!
//! When content gets its first report for a likely-illegal reason, an
//! evidence record is opened and the reported state is captured at once:
//! the item itself (with its images), its author and, for comments, what it
//! replied to -- nothing more, no likes, no other comments. A reported
//! direct message is preserved whatever the reason, with the ten messages
//! before it as context: its sender can delete it, or their account, at any
//! time, which erases it for the recipient too, and the team could otherwise
//! never see what was reported. From then on,
//! while the record is open, every edit of the item adds a version, and
//! deleting it (by moderation, by its author or with the account) only
//! marks the record, since the evidence is already safe. Reports for other
//! reasons (spam, impersonation, other) copy nothing.
//!
//! DB rows are written inside the caller's transaction (`capture`,
//! `preserve`). Files are copied into the evidence storage zone after the
//! commit (`finish`). An original file is never deleted while a copy of it
//! is still pending: a failed copy, or the zone not being configured yet,
//! leaves the original in place for the hourly sweeper to retry.
//!
//! A record is decided when the last likely-illegal report on its target
//! is resolved (`decide`): purged at the next sweep if all were dismissed,
//! kept for the retention period if any was actioned, and never purged
//! while a legal hold is set. See migrations/20260930010000_evidence.sql
//! for the tables and handlers/evidence.rs for the admin endpoints.
//!
//! The data export (Art. 15) deliberately doesn't include evidence records:
//! disclosing them to the reported person could compromise an
//! investigation (Art. 23 GDPR, §33 BDSG) -- pending legal review.

use std::time::Duration;

use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::errors::AppError;
use crate::handlers::auth::AppState;
use crate::utils::{delete_media, DbResultExt};

/// Report reasons that may point to a crime, so the reported content is
/// preserved. Spam, impersonation and "other" copy nothing. Kept as one
/// list so a lawyer's answer is a one-line change.
pub const LIKELY_ILLEGAL_REASONS: &[&str] = &[
    "csam", "violence", "hate_speech", "harassment", "sexual_content", "self_harm",
    "fraud", "ncii", "terrorism", "illegal_goods", "extremism",
];

pub fn is_likely_illegal(reason: &str) -> bool {
    LIKELY_ILLEGAL_REASONS.contains(&reason)
}

/// Whether a report on this kind of item for this reason preserves it:
/// likely-illegal reasons, and every report on a direct message (see the
/// module doc).
pub fn preserves(target_type: &str, reason: &str) -> bool {
    target_type == "message" || is_likely_illegal(reason)
}

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

/// What deleted the original.
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

/// Why `capture` is called.
#[derive(Clone, Copy)]
pub enum Cause {
    /// A likely-illegal report was just filed. Opens the record with the
    /// reported state; later reports on the same item share it.
    Reported,
    /// The item was just edited. Adds the new state, if a record is open.
    Edited,
    /// The item is about to be deleted. Marks the open record; only
    /// captures a version for reports filed before capture-on-report
    /// existed, which have no record yet.
    Deleted(Trigger),
    /// The moderation team removes the item. It stays, invisible, until the
    /// objection window has passed, so the record isn't marked deleted yet
    /// (`mark_deleted` does that later); a version is only captured when
    /// there is none, e.g. a spam report the team found to be illegal.
    Removed,
}

impl Cause {
    fn version_label(self) -> &'static str {
        match self {
            Cause::Reported => "reported",
            Cause::Edited => "edited",
            Cause::Deleted(_) | Cause::Removed => "deleted",
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
    pub messages: Vec<Uuid>,
    pub user: Option<Uuid>,
}

struct PendingFile {
    id: Uuid,
    source_key: String,
    storage_key: String,
}

/// Files captured inside a transaction, copied by `finish` after the
/// commit.
#[must_use]
#[derive(Default)]
pub struct Preserved {
    files: Vec<PendingFile>,
}

impl Preserved {
    pub fn extend(&mut self, other: Preserved) {
        self.files.extend(other.files);
    }
}

fn author_json(user_id_col: &str) -> String {
    format!(
        "(SELECT jsonb_build_object('id', u.id, 'username', u.username, 'display_name', u.display_name, \
         'email', u.email, 'created_at', u.created_at) FROM users u WHERE u.id = {user_id_col})"
    )
}

/// SELECT producing one version's content for the target with id $1: the
/// item, its author and (for comments) its context, nothing else.
fn snapshot_sql(target_type: &str) -> String {
    match target_type {
        "post" => format!(
            r#"
            SELECT jsonb_build_object(
                'post', jsonb_build_object('id', p.id, 'caption', p.caption, 'created_at', p.created_at,
                    'edited_at', p.edited_at, 'moderation_status', p.moderation_status),
                'author', {author})
            FROM posts p WHERE p.id = $1
            "#,
            author = author_json("p.user_id"),
        ),
        "comment" => format!(
            r#"
            SELECT jsonb_build_object(
                'comment', jsonb_build_object('id', c.id, 'body', c.body, 'created_at', c.created_at,
                    'edited_at', c.edited_at, 'moderation_status', c.moderation_status),
                'author', {author},
                'context', jsonb_build_object(
                    'post', (SELECT jsonb_build_object('id', p.id, 'caption', p.caption,
                        'author_id', p.user_id, 'created_at', p.created_at) FROM posts p WHERE p.id = c.post_id),
                    'parent_comment', (SELECT jsonb_build_object('id', pc.id, 'body', pc.body,
                        'author_id', pc.user_id, 'created_at', pc.created_at) FROM comments pc WHERE pc.id = c.parent_comment_id)))
            FROM comments c WHERE c.id = $1
            "#,
            author = author_json("c.user_id"),
        ),
        // The message, its sender, and the ten messages before it in the
        // conversation (both sides), so whoever reviews it sees what it
        // answered. Nothing else of the conversation is copied.
        "message" => format!(
            r#"
            SELECT jsonb_build_object(
                'message', jsonb_build_object('id', m.id, 'body', m.body, 'created_at', m.created_at,
                    'edited_at', m.edited_at, 'conversation_id', m.conversation_id,
                    'reply_to_message_id', m.reply_to_message_id),
                'author', {author},
                'recipient_id', (SELECT CASE WHEN c.user1_id = m.sender_id THEN c.user2_id ELSE c.user1_id END
                                 FROM conversations c WHERE c.id = m.conversation_id),
                'context', COALESCE((
                    SELECT jsonb_agg(jsonb_build_object('id', x.id, 'sender_id', x.sender_id, 'body', x.body,
                        'created_at', x.created_at, 'edited_at', x.edited_at) ORDER BY x.created_at, x.id)
                    FROM (SELECT p.id, p.sender_id, p.body, p.created_at, p.edited_at FROM messages p
                          WHERE p.conversation_id = m.conversation_id AND (p.created_at, p.id) < (m.created_at, m.id)
                          ORDER BY p.created_at DESC, p.id DESC LIMIT 10) x), '[]'::jsonb))
            FROM messages m WHERE m.id = $1
            "#,
            author = author_json("m.sender_id"),
        ),
        _ => format!(
            r#"
            SELECT jsonb_build_object(
                'profile', jsonb_build_object('id', u0.id, 'username', u0.username,
                    'display_name', u0.display_name, 'bio', u0.bio, 'created_at', u0.created_at),
                'author', {author})
            FROM users u0 WHERE u0.id = $1
            "#,
            author = author_json("u0.id"),
        ),
    }
}

/// Captures the target's current state into its open evidence record (see
/// `Cause` for what each call does). Runs inside the caller's transaction,
/// right after the report insert or the edit, or right before the delete.
pub async fn capture(
    tx: &mut PgConnection,
    target_type: &str,
    target_id: Uuid,
    cause: Cause,
    actor_id: Option<Uuid>,
) -> Result<Preserved, AppError> {
    let mut preserved = Preserved::default();

    let open = sqlx::query_scalar::<_, Uuid>(
        r#"
        SELECT id FROM evidence_records
        WHERE target_type = $1::report_target_type AND target_id = $2
          AND decided_at IS NULL AND purged_at IS NULL
        FOR UPDATE
        "#,
    )
    .bind(target_type)
    .bind(target_id)
    .fetch_optional(&mut *tx)
    .await
    .db_err_ctx("Evidence: finding open record failed", "Database error")?;

    let (evidence_id, created) = match (open, cause) {
        (Some(id), _) => (id, false),
        // An edit of something nobody reported as likely illegal.
        (None, Cause::Edited) => return Ok(preserved),
        (None, _) => {
            // DO NOTHING: a concurrent report opened it first, and that
            // transaction's snapshot is the reported state.
            let inserted = sqlx::query_scalar::<_, Uuid>(
                r#"
                INSERT INTO evidence_records (target_type, target_id)
                VALUES ($1::report_target_type, $2)
                ON CONFLICT (target_type, target_id) WHERE decided_at IS NULL AND purged_at IS NULL DO NOTHING
                RETURNING id
                "#,
            )
            .bind(target_type)
            .bind(target_id)
            .fetch_optional(&mut *tx)
            .await
            .db_err_ctx("Evidence: opening record failed", "Database error")?;
            match inserted {
                Some(id) => (id, true),
                None => return Ok(preserved),
            }
        }
    };

    sqlx::query(
        r#"
        UPDATE evidence_records SET reasons = (
            SELECT COALESCE(array_agg(DISTINCT r.reason::text), '{}') FROM reports r
            WHERE r.target_type = $2::report_target_type AND r.target_id = $3
              AND (r.target_type = 'message' OR r.reason::text = ANY($4))
        )
        WHERE id = $1
        "#,
    )
    .bind(evidence_id)
    .bind(target_type)
    .bind(target_id)
    .bind(likely_illegal())
    .execute(&mut *tx)
    .await
    .db_err_ctx("Evidence: updating reasons failed", "Database error")?;

    if created {
        log_event(tx, evidence_id, actor_id, "created", Some(cause.version_label()), None).await?;
        tracing::info!("Evidence {} opened for {} {}", evidence_id, target_type, target_id);
    }

    let add_version = match cause {
        Cause::Reported => created,
        Cause::Edited => true,
        Cause::Deleted(_) | Cause::Removed => {
            sqlx::query_scalar::<_, bool>("SELECT NOT EXISTS(SELECT 1 FROM evidence_versions WHERE evidence_id = $1)")
                .bind(evidence_id)
                .fetch_one(&mut *tx)
                .await
                .db_err("Database error")?
        }
    };
    if add_version {
        preserved.extend(add_version_row(tx, evidence_id, target_type, target_id, cause, actor_id).await?);
    }

    if let Cause::Deleted(trigger) = cause {
        sqlx::query("UPDATE evidence_records SET content_deleted_at = NOW(), deletion_trigger = $2 WHERE id = $1")
            .bind(evidence_id)
            .bind(trigger.as_str())
            .execute(&mut *tx)
            .await
            .db_err_ctx("Evidence: marking deletion failed", "Database error")?;
        log_event(tx, evidence_id, actor_id, "content_deleted", Some(trigger.as_str()), None).await?;
    }

    Ok(preserved)
}

async fn add_version_row(
    tx: &mut PgConnection,
    evidence_id: Uuid,
    target_type: &str,
    target_id: Uuid,
    cause: Cause,
    actor_id: Option<Uuid>,
) -> Result<Preserved, AppError> {
    let mut preserved = Preserved::default();

    let sql = format!(
        "INSERT INTO evidence_versions (evidence_id, cause, content) \
         SELECT $2, $3, snap.content FROM ({}) AS snap(content) RETURNING id",
        snapshot_sql(target_type)
    );
    // None: the target is already gone, so there's nothing to capture.
    let Some(version_id) = sqlx::query_scalar::<_, Uuid>(&sql)
        .bind(target_id)
        .bind(evidence_id)
        .bind(cause.version_label())
        .fetch_optional(&mut *tx)
        .await
        .db_err_ctx("Evidence: saving version failed", "Database error")?
    else {
        return Ok(preserved);
    };

    match target_type {
        "post" => {
            // Post images can't be edited, so they're captured once, with
            // the first version. FOR UPDATE: a concurrent media key rotation
            // (reports.rs, on a CSAM hide) waits for this transaction.
            let already = sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM evidence_files WHERE evidence_id = $1)")
                .bind(evidence_id)
                .fetch_one(&mut *tx)
                .await
                .db_err("Database error")?;
            if !already {
                let media = sqlx::query_as::<_, (String, i32)>(
                    "SELECT full_key, sort_order FROM media_assets WHERE post_id = $1 ORDER BY sort_order FOR UPDATE",
                )
                .bind(target_id)
                .fetch_all(&mut *tx)
                .await
                .db_err_ctx("Evidence: reading post media failed", "Database error")?;
                for (key, sort_order) in media {
                    add_file(tx, &mut preserved, evidence_id, version_id, "post_media", sort_order, key).await?;
                }
            }
        }
        "user" => {
            let avatar = sqlx::query_scalar::<_, Option<String>>("SELECT avatar_url FROM users WHERE id = $1")
                .bind(target_id)
                .fetch_one(&mut *tx)
                .await
                .db_err_ctx("Evidence: reading avatar failed", "Database error")?;
            if let Some(avatar) = avatar {
                // Older rows carry a "/media/" prefix (see delete_account).
                let key = avatar.strip_prefix("/media/").unwrap_or(&avatar).to_string();
                // Each avatar upload gets a new key, so an unchanged key
                // means an unchanged picture that's already captured.
                let already = sqlx::query_scalar::<_, bool>(
                    "SELECT EXISTS(SELECT 1 FROM evidence_files WHERE evidence_id = $1 AND source_key = $2)",
                )
                .bind(evidence_id)
                .bind(&key)
                .fetch_one(&mut *tx)
                .await
                .db_err("Database error")?;
                if !already {
                    add_file(tx, &mut preserved, evidence_id, version_id, "avatar", 0, key).await?;
                }
            }
        }
        _ => {}
    }

    log_event(
        tx,
        evidence_id,
        actor_id,
        "version_added",
        Some(cause.version_label()),
        Some(json!({ "version_id": version_id })),
    )
    .await?;
    Ok(preserved)
}

/// Before a delete: marks (or, for older reports, creates) the evidence of
/// everything in `scope` that has a pending likely-illegal report. Runs
/// inside the caller's transaction, before its delete.
pub async fn preserve(
    tx: &mut PgConnection,
    scope: &Scope,
    trigger: Trigger,
    actor_id: Option<Uuid>,
) -> Result<Preserved, AppError> {
    let reasons = likely_illegal();
    let mut preserved = Preserved::default();

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

    // Every pending report on a message preserves it (see `preserves`).
    let messages = sqlx::query_scalar::<_, Uuid>(
        r#"
        SELECT m.id FROM messages m
        WHERE m.id = ANY($1) AND EXISTS (
            SELECT 1 FROM reports r WHERE r.target_type = 'message' AND r.target_id = m.id AND r.status = 'pending'
        )
        "#,
    )
    .bind(&scope.messages)
    .fetch_all(&mut *tx)
    .await
    .db_err_ctx("Evidence: finding reported messages failed", "Database error")?;

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

    let cause = Cause::Deleted(trigger);
    for post_id in posts {
        preserved.extend(capture(tx, "post", post_id, cause, actor_id).await?);
    }
    for comment_id in comments {
        preserved.extend(capture(tx, "comment", comment_id, cause, actor_id).await?);
    }
    for message_id in messages {
        preserved.extend(capture(tx, "message", message_id, cause, actor_id).await?);
    }
    if let Some(user_id) = user {
        preserved.extend(capture(tx, "user", user_id, cause, actor_id).await?);
    }

    Ok(preserved)
}

/// On a removal: preserves the target when the team classified it as a
/// likely-illegal violation, even if no report gave a likely-illegal reason,
/// and sets the record's authority-report advice. A record opened at report
/// time already holds the reported state; then this only adds the reason
/// and the advice.
pub async fn preserve_classified(
    tx: &mut PgConnection,
    target_type: &str,
    target_id: Uuid,
    reason: &str,
    authority_report: Option<&str>,
    admin_id: Uuid,
) -> Result<Preserved, AppError> {
    if !preserves(target_type, reason) || !matches!(target_type, "post" | "comment" | "message" | "user") {
        return Ok(Preserved::default());
    }
    let preserved = capture(tx, target_type, target_id, Cause::Removed, Some(admin_id)).await?;

    // capture() derives `reasons` from the reports; add the classified one.
    let updated = sqlx::query_scalar::<_, Uuid>(
        r#"
        UPDATE evidence_records
        SET reasons = CASE WHEN $3 = ANY(reasons) THEN reasons ELSE array_append(reasons, $3) END,
            authority_report = CASE
                WHEN authority_report = 'required' OR $4::text = 'required' THEN 'required'
                ELSE COALESCE($4, authority_report) END
        WHERE target_type = $1::report_target_type AND target_id = $2 AND decided_at IS NULL AND purged_at IS NULL
        RETURNING id
        "#,
    )
    .bind(target_type)
    .bind(target_id)
    .bind(reason)
    .bind(authority_report)
    .fetch_optional(&mut *tx)
    .await
    .db_err_ctx("Evidence: recording classification failed", "Database error")?;
    if let (Some(evidence_id), Some(advice)) = (updated, authority_report) {
        log_event(tx, evidence_id, Some(admin_id), "authority_report_advised", None, Some(json!({ "advice": advice })))
            .await?;
    }
    Ok(preserved)
}

#[allow(clippy::too_many_arguments)]
async fn add_file(
    tx: &mut PgConnection,
    preserved: &mut Preserved,
    evidence_id: Uuid,
    version_id: Uuid,
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
        INSERT INTO evidence_files (id, evidence_id, version_id, kind, sort_order, source_key, storage_key, content_type)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        "#,
    )
    .bind(id)
    .bind(evidence_id)
    .bind(version_id)
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

/// After the caller's commit: copies the captured files into the evidence
/// zone, then releases `media_keys` (files the caller no longer needs, e.g.
/// of deleted content or a replaced avatar).
pub async fn finish(state: &AppState, preserved: Preserved, media_keys: impl IntoIterator<Item = String>) {
    for file in &preserved.files {
        if let Err(e) = copy_file(state, file.id, &file.source_key, &file.storage_key).await {
            tracing::error!(
                "Evidence file {} not copied yet, the sweeper retries from {}: {}",
                file.id, file.source_key, e.message
            );
        }
    }
    for key in media_keys {
        release_media(state, &key).await;
    }
}

/// Deletes a media file the caller is done with -- unless a pending
/// evidence copy still needs it, in which case only its CDN cache is
/// purged and the sweeper deletes it once the copy has succeeded.
pub async fn release_media(state: &AppState, key: &str) {
    let pending = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM evidence_files WHERE source_key = $1 AND copied_at IS NULL)",
    )
    .bind(key)
    .fetch_one(&state.db)
    .await;

    match pending {
        Ok(false) => delete_media(state, key).await,
        Ok(true) | Err(_) => {
            if let Err(e) = &pending {
                // Unsure, so keep it: an orphaned file is recoverable, lost
                // evidence isn't.
                tracing::error!("Keeping media {}: pending-evidence check failed: {}", key, e);
            }
            if let Err(e) = state.cdn.purge(&state.storage.public_url(key)).await {
                tracing::error!("Failed to purge media {} from CDN: {}", key, e.message);
            }
        }
    }
}

/// Whether anything live still uses a media file: a post's media or a
/// user's avatar, or a pending evidence copy outside `except_evidence`.
async fn source_still_needed(state: &AppState, key: &str, except_evidence: Option<Uuid>) -> Result<bool, AppError> {
    sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS(SELECT 1 FROM media_assets WHERE $1 IN (thumb_key, medium_key, full_key, original_key))
            OR EXISTS(SELECT 1 FROM users WHERE avatar_url = $1 OR avatar_url = '/media/' || $1)
            OR EXISTS(SELECT 1 FROM evidence_files
                      WHERE source_key = $1 AND copied_at IS NULL AND evidence_id IS DISTINCT FROM $2)
        "#,
    )
    .bind(key)
    .bind(except_evidence)
    .fetch_one(&state.db)
    .await
    .db_err("Database error")
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

/// A CSAM hide moves a post's files to new keys (reports.rs). Pending
/// copies follow them, so the sweeper doesn't retry from a deleted key.
pub async fn follow_moved_source(state: &AppState, old_key: &str, new_key: &str) -> Result<(), AppError> {
    sqlx::query("UPDATE evidence_files SET source_key = $2 WHERE source_key = $1 AND copied_at IS NULL")
        .bind(old_key)
        .bind(new_key)
        .execute(&state.db)
        .await
        .db_err_ctx("Evidence: updating moved source failed", "Database error")?;
    Ok(())
}

/// Called whenever a report is resolved, inside the resolving
/// transaction. Once no likely-illegal report on the target is pending any
/// more, its open record gets its decision: 'removed' (kept for the
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
                AND r.status = 'pending' AND (r.target_type = 'message' OR r.reason::text = ANY($6))
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

/// After an accepted objection reversed a removal: the content wasn't a
/// violation after all, so its evidence is purged at the next sweep like a
/// dismissed report's -- unless a legal hold is set or a report to the
/// authorities was recorded, which keep it whatever the objection says.
pub async fn dismiss_after_objection(
    tx: &mut PgConnection,
    target_type: &str,
    target_id: Uuid,
    admin_id: Uuid,
) -> Result<(), AppError> {
    let rows = sqlx::query_scalar::<_, Uuid>(
        r#"
        UPDATE evidence_records e
        SET decision = 'dismissed', decided_at = COALESCE(decided_at, NOW()), decided_by = COALESCE(decided_by, $3),
            retain_until = NOW()
        WHERE e.target_type = $1::report_target_type AND e.target_id = $2 AND e.purged_at IS NULL
          AND e.decision = 'removed' AND NOT e.legal_hold
          AND NOT EXISTS (SELECT 1 FROM evidence_events ev WHERE ev.evidence_id = e.id AND ev.action = 'authority_report')
        RETURNING e.id
        "#,
    )
    .bind(target_type)
    .bind(target_id)
    .bind(admin_id)
    .fetch_all(&mut *tx)
    .await
    .db_err_ctx("Evidence: recording the objection failed", "Database error")?;
    for evidence_id in rows {
        log_event(tx, evidence_id, Some(admin_id), "decided", Some("objection accepted"), Some(json!({ "decision": "dismissed" })))
            .await?;
    }
    Ok(())
}

/// Removed content is kept, invisible, until the objection window has
/// passed (see moderation::purge_removed); when it is finally deleted, its
/// evidence record notes that, like any other deletion.
pub async fn mark_deleted(
    tx: &mut PgConnection,
    target_type: &str,
    target_id: Uuid,
    trigger: Trigger,
) -> Result<(), AppError> {
    let rows = sqlx::query_scalar::<_, Uuid>(
        r#"
        UPDATE evidence_records SET content_deleted_at = NOW(), deletion_trigger = $3
        WHERE target_type = $1::report_target_type AND target_id = $2 AND purged_at IS NULL AND content_deleted_at IS NULL
        RETURNING id
        "#,
    )
    .bind(target_type)
    .bind(target_id)
    .bind(trigger.as_str())
    .fetch_all(&mut *tx)
    .await
    .db_err_ctx("Evidence: marking deletion failed", "Database error")?;
    for evidence_id in rows {
        log_event(tx, evidence_id, None, "content_deleted", Some(trigger.as_str()), None).await?;
    }
    Ok(())
}

/// Runs the sweep once an hour on every replica. The row locks in each
/// step (SKIP LOCKED) keep replicas from working on the same record.
pub fn spawn_sweeper(state: AppState) {
    tokio::spawn(async move {
        loop {
            sweep(&state).await;
            tokio::time::sleep(Duration::from_secs(60 * 60)).await;
        }
    });
}

/// Grace period for deciding: an evidence record (i.e. a likely-illegal
/// report) still undecided after this many days is overdue. It is listed
/// first in the admin views and logged on every sweep. Nothing is purged
/// automatically -- unreviewed evidence of a possible crime must not
/// silently disappear -- so the fix is a decision on the report.
pub const OVERDUE_DAYS: i32 = 30;

pub(crate) async fn sweep(state: &AppState) {
    retry_pending_copies(state).await;
    purge_due(state).await;
    warn_overdue(state).await;
}

async fn warn_overdue(state: &AppState) {
    match sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*) FROM evidence_records
        WHERE decided_at IS NULL AND purged_at IS NULL AND created_at < NOW() - make_interval(days => $1)
        "#,
    )
    .bind(OVERDUE_DAYS)
    .fetch_one(&state.db)
    .await
    {
        Ok(0) => {}
        Ok(n) => tracing::warn!(
            "{} evidence record(s) undecided for more than {} days -- their reports need a decision",
            n, OVERDUE_DAYS
        ),
        Err(e) => tracing::error!("Evidence sweep: counting overdue records failed: {}", e),
    }
}

/// Copies files whose copy failed, then deletes originals nothing live
/// uses any more. Skips anything younger than five minutes, which `finish`
/// is most likely still working on.
async fn retry_pending_copies(state: &AppState) {
    if !state.evidence.is_configured() {
        return;
    }
    let pending = match sqlx::query_as::<_, (Uuid, Uuid, String, String)>(
        r#"
        SELECT f.id, f.evidence_id, f.source_key, f.storage_key
        FROM evidence_files f JOIN evidence_versions v ON v.id = f.version_id
        WHERE f.copied_at IS NULL AND v.captured_at < NOW() - INTERVAL '5 minutes'
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

    for (file_id, evidence_id, source_key, storage_key) in pending {
        if let Err(e) = copy_file(state, file_id, &source_key, &storage_key).await {
            tracing::error!("Evidence sweep: file {} still not copied: {}", file_id, e.message);
            continue;
        }
        tracing::info!("Evidence sweep: copied file {}", file_id);
        match source_still_needed(state, &source_key, Some(evidence_id)).await {
            Ok(false) => delete_media(state, &source_key).await,
            Ok(true) => {}
            Err(e) => tracing::error!("Evidence sweep: keeping {}: {}", source_key, e.message),
        }
    }
}

/// Purges records whose retention has passed and that aren't on hold: the
/// versions and files are deleted and a 'purged' event is logged.
async fn purge_due(state: &AppState) {
    let due = match sqlx::query_scalar::<_, Uuid>(
        r#"
        SELECT id FROM evidence_records
        WHERE purged_at IS NULL AND NOT legal_hold AND retain_until <= NOW()
          -- A required report to the authorities that nobody recorded yet
          -- keeps the evidence (DSA Art. 18); it is listed first instead.
          -- IS DISTINCT FROM, since most records have no advice (NULL).
          AND (authority_report IS DISTINCT FROM 'required' OR EXISTS (
              SELECT 1 FROM evidence_events ev WHERE ev.evidence_id = evidence_records.id AND ev.action = 'authority_report'))
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
        } else if !source_still_needed(state, &source_key, Some(evidence_id)).await? {
            // Never copied, and the content is gone: the original is the
            // only copy. Live content (a dismissed report) keeps its file.
            delete_media(state, &source_key).await;
        }
    }

    sqlx::query("DELETE FROM evidence_files WHERE evidence_id = $1")
        .bind(evidence_id)
        .execute(&mut *tx)
        .await
        .db_err("Database error")?;
    sqlx::query("DELETE FROM evidence_versions WHERE evidence_id = $1")
        .bind(evidence_id)
        .execute(&mut *tx)
        .await
        .db_err("Database error")?;
    sqlx::query("UPDATE evidence_records SET purged_at = NOW() WHERE id = $1")
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
