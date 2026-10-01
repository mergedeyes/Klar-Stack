//! Deleting what Klar no longer needs to keep, once an hour on every
//! replica (row locks with SKIP LOCKED keep replicas apart).
//!
//! - Reports: six months after their decision, unless an active strike,
//!   preserved evidence or a pending objection still rests on them; notices
//!   from the public form go with their report.
//! - Decisions: three years after the decision, its lifting or the answer
//!   to an objection (the regular limitation period, § 195 BGB, like rights
//!   claims), unless an active strike or a current suspension rests on them.
//! - Notifications: read ones after 90 days, unread ones after a year.
//! - Accounts that never verified their address: 30 days after sign-up, a
//!   week after a reminder with a fresh link.
//! - Expired refresh tokens.
//!
//! ⚖️ All periods are pending legal review.
//!
//! Removed content: a post or comment the moderation team removed stays in
//! the database, invisible to everyone, until the objection window has
//! passed, so an accepted objection can restore it (DSA Art. 20(4) asks for
//! decisions to be reversed when an objection succeeds). Then it is deleted
//! for good, with its files. Child sexual abuse material doesn't wait: it is
//! deleted as soon as its evidence copy is complete. A removed comment that
//! others replied to keeps an empty placeholder, so their replies stay.
//! ⚖️ Keeping removed content for the objection window (storage limitation
//! against restorability) is pending legal review.

use std::time::Duration;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::errors::AppError;
use crate::evidence;
use crate::handlers::auth::{create_verification_token, AppState};
use crate::handlers::posts::delete_post_with_media;
use crate::moderation;
use crate::utils::DbResultExt;

/// How long removed content is kept for a possible restoration: the
/// objection window (see handlers/moderation.rs).
pub const REMOVED_RETENTION_DAYS: i32 = 183;

pub fn spawn(state: AppState) {
    tokio::spawn(async move {
        loop {
            sweep(&state).await;
            tokio::time::sleep(Duration::from_secs(60 * 60)).await;
        }
    });
}

pub(crate) async fn sweep(state: &AppState) {
    purge_removed(state).await;
    sweep_unverified(state).await;
    delete_expired_records(state).await;
}

pub const REPORT_RETENTION_DAYS: i32 = 183;
pub const DECISION_RETENTION_DAYS: i32 = 3 * 365;
pub const READ_NOTIFICATION_DAYS: i32 = 90;
pub const UNREAD_NOTIFICATION_DAYS: i32 = 365;
pub const UNVERIFIED_DELETION_DAYS: i32 = 30;
/// The reminder goes out this many days before the deletion; its link is
/// valid as long.
pub const UNVERIFIED_REMINDER_DAYS: i32 = 7;

/// Records past their retention period; see the module doc.
pub(crate) async fn delete_expired_records(state: &AppState) {
    let steps: [(&str, &str, &[i32]); 5] = [
        (
            "reports",
            r#"
            DELETE FROM reports r
            WHERE r.status != 'pending' AND r.reviewed_at < NOW() - make_interval(days => $1)
              AND NOT EXISTS (SELECT 1 FROM moderation_decisions d JOIN account_strikes s ON s.decision_id = d.id
                              WHERE r.id = ANY(d.report_ids) AND (s.expires_at IS NULL OR s.expires_at > NOW()))
              AND NOT EXISTS (SELECT 1 FROM moderation_decisions d
                              WHERE r.id = ANY(d.report_ids) AND d.objection_status = 'pending')
              AND NOT EXISTS (SELECT 1 FROM evidence_records e
                              WHERE e.target_type = r.target_type AND e.target_id = r.target_id AND e.purged_at IS NULL)
            "#,
            &[REPORT_RETENTION_DAYS],
        ),
        (
            "notices from the public form",
            r#"
            DELETE FROM content_notices n
            WHERE COALESCE(n.decided_at, n.created_at) < NOW() - make_interval(days => $1)
              AND NOT EXISTS (SELECT 1 FROM reports r WHERE r.notice_id = n.id)
            "#,
            &[REPORT_RETENTION_DAYS],
        ),
        (
            "decisions",
            r#"
            DELETE FROM moderation_decisions d
            WHERE GREATEST(d.created_at, COALESCE(d.lifted_at, d.created_at), COALESCE(d.objection_resolved_at, d.created_at))
                  < NOW() - make_interval(days => $1)
              AND d.objection_status IS DISTINCT FROM 'pending'
              AND (d.restriction != 'removed' OR d.content_purged_at IS NOT NULL)
              AND NOT EXISTS (SELECT 1 FROM account_strikes s
                              WHERE s.decision_id = d.id AND (s.expires_at IS NULL OR s.expires_at > NOW()))
              AND NOT EXISTS (SELECT 1 FROM users u WHERE u.suspension_decision_id = d.id AND u.suspended_until > NOW())
            "#,
            &[DECISION_RETENTION_DAYS],
        ),
        (
            "notifications",
            r#"
            DELETE FROM notifications
            WHERE (is_read AND created_at < NOW() - make_interval(days => $1))
               OR created_at < NOW() - make_interval(days => $2)
            "#,
            &[READ_NOTIFICATION_DAYS, UNREAD_NOTIFICATION_DAYS],
        ),
        ("expired refresh tokens", "DELETE FROM refresh_tokens WHERE expires_at < NOW()", &[]),
    ];
    for (what, sql, binds) in steps {
        let mut query = sqlx::query(sql);
        for bind in binds {
            query = query.bind(*bind);
        }
        match query.execute(&state.db).await {
            Ok(r) if r.rows_affected() > 0 => tracing::info!("Retention: deleted {} {}", r.rows_affected(), what),
            Ok(_) => {}
            Err(e) => tracing::error!("Retention: deleting {} failed: {}", what, e),
        }
    }
}

/// Accounts that never verified their address: a reminder with a fresh
/// link a week before, then the deletion. Each reminder is claimed before
/// it is sent, so replicas don't send two.
pub(crate) async fn sweep_unverified(state: &AppState) {
    let due = sqlx::query_as::<_, (Uuid, String, String, DateTime<Utc>)>(
        r#"
        UPDATE users SET verification_reminder_at = NOW()
        WHERE id IN (
            SELECT id FROM users
            WHERE NOT email_verified AND verification_reminder_at IS NULL
              AND created_at <= NOW() - make_interval(days => $1)
            ORDER BY created_at
            LIMIT 100
            FOR UPDATE SKIP LOCKED
        )
        RETURNING id, email, username, created_at
        "#,
    )
    .bind(UNVERIFIED_DELETION_DAYS - UNVERIFIED_REMINDER_DAYS)
    .fetch_all(&state.db)
    .await;
    match due {
        Ok(due) => {
            for (user_id, email, username, created_at) in due {
                if let Err(e) = remind_unverified(state, user_id, &email, &username, created_at).await {
                    tracing::error!("Retention: verification reminder for user {} failed: {}", user_id, e.message);
                }
            }
        }
        Err(e) => tracing::error!("Retention: listing unverified accounts failed: {}", e),
    }

    // A full reminder period after the reminder, also for accounts that
    // were older than the deletion age when this rule began.
    let doomed = sqlx::query_scalar::<_, Uuid>(
        r#"
        SELECT id FROM users
        WHERE NOT email_verified AND created_at <= NOW() - make_interval(days => $1)
          AND verification_reminder_at <= NOW() - make_interval(days => $2)
        ORDER BY created_at
        LIMIT 50
        "#,
    )
    .bind(UNVERIFIED_DELETION_DAYS)
    .bind(UNVERIFIED_REMINDER_DAYS)
    .fetch_all(&state.db)
    .await;
    match doomed {
        Ok(doomed) => {
            for user_id in doomed {
                if let Err(e) = crate::handlers::users::delete_user(state, user_id, crate::handlers::users::Deletion::Unverified).await {
                    tracing::error!("Retention: deleting unverified account {} failed: {}", user_id, e.message);
                }
            }
        }
        Err(e) => tracing::error!("Retention: listing unverified accounts failed: {}", e),
    }
}

async fn remind_unverified(
    state: &AppState,
    user_id: Uuid,
    email: &str,
    username: &str,
    created_at: DateTime<Utc>,
) -> Result<(), AppError> {
    let mut conn = state.db.acquire().await.db_err("Database error")?;
    let token = create_verification_token(&mut conn, user_id, UNVERIFIED_REMINDER_DAYS * 24).await?;
    let planned = created_at + chrono::Duration::days(UNVERIFIED_DELETION_DAYS as i64);
    let earliest = Utc::now() + chrono::Duration::days(UNVERIFIED_REMINDER_DAYS as i64);
    let on = planned.max(earliest).format("%d.%m.%Y").to_string();
    state
        .email
        .send_verification_reminder(email, username, &token, &on)
        .await
        .map_err(|e| AppError::internal(e.0))
}

/// Removals whose content is due for deletion: the window has passed, or
/// it is CSAM with a complete evidence copy. Never while an objection is
/// pending: its outcome may restore the content.
const DUE_REMOVALS: &str = r#"
    SELECT d.id FROM moderation_decisions d
    WHERE d.restriction = 'removed' AND d.content_purged_at IS NULL AND d.lifted_at IS NULL AND d.superseded_by IS NULL
      AND d.target_type IN ('post', 'comment', 'user')
      AND d.objection_status IS DISTINCT FROM 'pending'
      AND (d.created_at <= NOW() - make_interval(days => $1)
           OR (d.reason = 'csam'
               AND EXISTS (SELECT 1 FROM evidence_records e
                           WHERE e.target_type = d.target_type AND e.target_id = d.target_id AND e.purged_at IS NULL)
               AND NOT EXISTS (SELECT 1 FROM evidence_files f JOIN evidence_records e ON e.id = f.evidence_id
                               WHERE e.target_type = d.target_type AND e.target_id = d.target_id AND f.copied_at IS NULL)))
    ORDER BY d.created_at
    LIMIT 50
"#;

pub(crate) async fn purge_removed(state: &AppState) {
    let due = match sqlx::query_scalar::<_, Uuid>(DUE_REMOVALS)
        .bind(REMOVED_RETENTION_DAYS)
        .fetch_all(&state.db)
        .await
    {
        Ok(ids) => ids,
        Err(e) => {
            tracing::error!("Retention: listing removed content failed: {}", e);
            return;
        }
    };
    for decision_id in due {
        if let Err(e) = purge_one(state, decision_id).await {
            tracing::error!("Retention: deleting the content of decision {} failed, retrying next sweep: {}", decision_id, e.message);
        }
    }

    // Placeholders of removed comments whose replies are all gone too.
    if let Err(e) = sqlx::query(
        r#"
        DELETE FROM comments c
        WHERE c.moderation_status = 'removed' AND c.body = ''
          AND NOT EXISTS (SELECT 1 FROM comments r WHERE r.parent_comment_id = c.id)
          AND EXISTS (SELECT 1 FROM moderation_decisions d
                      WHERE d.target_type = 'comment' AND d.target_id = c.id AND d.content_purged_at IS NOT NULL)
        "#,
    )
    .execute(&state.db)
    .await
    {
        tracing::error!("Retention: deleting comment placeholders failed: {}", e);
    }
}

async fn purge_one(state: &AppState, decision_id: Uuid) -> Result<(), AppError> {
    let mut tx = state.db.begin().await.db_err("Database error")?;
    let Some((target_type, target_id)) = sqlx::query_as::<_, (String, Uuid)>(
        r#"
        SELECT target_type::text, target_id FROM moderation_decisions
        WHERE id = $1 AND content_purged_at IS NULL AND lifted_at IS NULL AND superseded_by IS NULL
          AND objection_status IS DISTINCT FROM 'pending'
        FOR UPDATE SKIP LOCKED
        "#,
    )
    .bind(decision_id)
    .fetch_optional(&mut *tx)
    .await
    .db_err("Database error")?
    else {
        return Ok(());
    };

    let mut media_keys = Vec::new();
    let mut preserved = evidence::Preserved::default();
    let mut notices = moderation::PendingNotices::default();
    match target_type.as_str() {
        // A profile picture removed by the team, kept for a possible
        // objection (profile_moderation.rs).
        "user" => {
            let key = sqlx::query_scalar::<_, Option<String>>(
                "SELECT removed_fields->>'avatar_key' FROM moderation_decisions WHERE id = $1",
            )
            .bind(decision_id)
            .fetch_one(&mut *tx)
            .await
            .db_err("Database error")?;
            media_keys.extend(key);
        }
        "post" => {
            let removed = sqlx::query_scalar::<_, bool>("SELECT moderation_status = 'removed' FROM posts WHERE id = $1 FOR UPDATE")
                .bind(target_id)
                .fetch_optional(&mut *tx)
                .await
                .db_err("Database error")?;
            if removed == Some(true) {
                // The comments under it go with it: preserve the reported
                // ones like any other deletion, and close reports nobody
                // needs to decide any more.
                let scope = evidence::Scope { posts: vec![target_id], ..Default::default() };
                preserved = evidence::preserve(&mut tx, &scope, evidence::Trigger::ModerationRemoval, None).await?;
                let comments = sqlx::query_scalar::<_, Uuid>("SELECT id FROM comments WHERE post_id = $1")
                    .bind(target_id)
                    .fetch_all(&mut *tx)
                    .await
                    .db_err("Database error")?;
                notices.extend(moderation::close_obsolete_reports(&mut tx, "comment", &comments).await?);
                evidence::mark_deleted(&mut tx, "post", target_id, evidence::Trigger::ModerationRemoval).await?;
                media_keys = delete_post_with_media(&mut tx, target_id).await?.unwrap_or_default();
            }
        }
        _ => {
            let comment = sqlx::query_as::<_, (bool, bool)>(
                r#"
                SELECT moderation_status = 'removed', EXISTS(SELECT 1 FROM comments r WHERE r.parent_comment_id = c.id)
                FROM comments c WHERE c.id = $1 FOR UPDATE
                "#,
            )
            .bind(target_id)
            .fetch_optional(&mut *tx)
            .await
            .db_err("Database error")?;
            if let Some((true, has_replies)) = comment {
                evidence::mark_deleted(&mut tx, "comment", target_id, evidence::Trigger::ModerationRemoval).await?;
                let sql = if has_replies {
                    "UPDATE comments SET body = '' WHERE id = $1"
                } else {
                    "DELETE FROM comments WHERE id = $1"
                };
                sqlx::query(sql).bind(target_id).execute(&mut *tx).await.db_err("Database error")?;
            }
        }
    }

    sqlx::query("UPDATE moderation_decisions SET content_purged_at = NOW() WHERE id = $1")
        .bind(decision_id)
        .execute(&mut *tx)
        .await
        .db_err("Database error")?;
    tx.commit().await.db_err("Database error")?;
    notices.send(state).await;
    evidence::finish(state, preserved, media_keys).await;
    tracing::info!("Retention: removed {} {} deleted for good (decision {})", target_type, target_id, decision_id);
    Ok(())
}
