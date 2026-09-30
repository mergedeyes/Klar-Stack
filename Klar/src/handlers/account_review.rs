//! Account reviews: one page with an account's recent activity, for
//! deciding whether it was taken over, is a bot or spam-only account, or
//! is fine (see the account_review migration).
//!
//! Accounts come up for review from a report (a "Review account" button in
//! the queue), from the admin standing and security pages, or from the
//! "Needs review" list, which flags accounts by signals computed from data
//! Klar already stores -- bursts of activity, repeated identical texts,
//! sudden activity after a long silence. Nothing is locked or banned
//! automatically, and no IPs or devices are collected for this.
//!
//! Direct messages appear as numbers only (sent, distinct recipients),
//! never their content: an account looking odd is no reason to read
//! private conversations. Reported messages reach moderation through
//! reports.

use axum::{
    extract::{Path, State},
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::errors::AppError;
use crate::handlers::account_lock::{checked_text, lock_user};
use crate::handlers::auth::AppState;
use crate::handlers::reports::require_admin;
use crate::moderation;
use crate::standing::Measure;
use crate::utils::DbResultExt;

/// Signal thresholds. A burst counts posts, comments and messages sent in
/// the same ten-minute window over the last week.
const BURST_FLAG: i64 = 10;
const DUPLICATES_FLAG: i64 = 3;
/// Activity in the last day after this many days without any...
const DORMANT_DAYS: i32 = 60;
/// ...counts when it is at least this much.
const DORMANT_ACTIVITY_FLAG: i64 = 5;
const LIST_LIMIT: i64 = 30;

#[derive(Debug, Serialize, sqlx::FromRow, Default)]
pub struct Signals {
    /// Most posts, comments and messages in one ten-minute window, last 7 days.
    pub max_burst: i64,
    /// Most posts or comments with the same text, last 7 days.
    pub max_duplicates: i64,
    /// Posts and comments with a link, last 7 days.
    pub links: i64,
    /// Posts, comments and messages in the last 24 hours.
    pub activity_24h: i64,
    /// Active in the last day after DORMANT_DAYS of silence (account older
    /// than that).
    pub woke_up: bool,
}

impl Signals {
    /// The signals worth a review, as short labels.
    pub fn flags(&self) -> Vec<&'static str> {
        let mut flags = Vec::new();
        if self.max_burst >= BURST_FLAG {
            flags.push("burst");
        }
        if self.max_duplicates >= DUPLICATES_FLAG {
            flags.push("duplicates");
        }
        if self.woke_up && self.activity_24h >= DORMANT_ACTIVITY_FLAG {
            flags.push("woke_up");
        }
        flags
    }
}

async fn signals(conn: &mut PgConnection, user_id: Uuid) -> Result<Signals, AppError> {
    sqlx::query_as::<_, Signals>(
        r#"
        WITH act AS (
            SELECT created_at FROM posts WHERE user_id = $1 AND created_at > NOW() - INTERVAL '7 days'
            UNION ALL
            SELECT created_at FROM comments WHERE user_id = $1 AND created_at > NOW() - INTERVAL '7 days'
            UNION ALL
            SELECT created_at FROM messages WHERE sender_id = $1 AND created_at > NOW() - INTERVAL '7 days'
        ),
        texts AS (
            SELECT caption AS t FROM posts WHERE user_id = $1 AND created_at > NOW() - INTERVAL '7 days'
            UNION ALL
            SELECT body FROM comments WHERE user_id = $1 AND created_at > NOW() - INTERVAL '7 days'
        ),
        before_today AS (
            SELECT MAX(at) AS at FROM (
                SELECT MAX(created_at) AS at FROM posts WHERE user_id = $1 AND created_at <= NOW() - INTERVAL '1 day'
                UNION ALL
                SELECT MAX(created_at) FROM comments WHERE user_id = $1 AND created_at <= NOW() - INTERVAL '1 day'
                UNION ALL
                SELECT MAX(created_at) FROM messages WHERE sender_id = $1 AND created_at <= NOW() - INTERVAL '1 day'
            ) x
        )
        SELECT
            COALESCE((SELECT MAX(n) FROM (
                SELECT COUNT(*) AS n FROM act GROUP BY date_bin('10 minutes', created_at, TIMESTAMPTZ '2000-01-01')
            ) b), 0) AS max_burst,
            COALESCE((SELECT MAX(n) FROM (
                SELECT COUNT(*) AS n FROM texts WHERE length(trim(t)) > 0 GROUP BY lower(trim(t))
            ) d), 0) AS max_duplicates,
            (SELECT COUNT(*) FROM texts WHERE t ~* 'https?://') AS links,
            (SELECT COUNT(*) FROM act WHERE created_at > NOW() - INTERVAL '1 day') AS activity_24h,
            (EXISTS (SELECT 1 FROM act WHERE created_at > NOW() - INTERVAL '1 day')
             AND (SELECT created_at FROM users WHERE id = $1) < NOW() - make_interval(days => $2)
             AND COALESCE((SELECT at FROM before_today) < NOW() - make_interval(days => $2), TRUE)) AS woke_up
        "#,
    )
    .bind(user_id)
    .bind(DORMANT_DAYS)
    .fetch_one(&mut *conn)
    .await
    .db_err("Database error")
}

#[derive(Debug, Serialize)]
pub struct Candidate {
    pub username: String,
    pub signals: Signals,
    pub flags: Vec<&'static str>,
    /// Pending spam, fraud or impersonation reports on the account or its
    /// content: the report reasons that most often mean a hijacked or bot
    /// account.
    pub spam_reports: i64,
    pub locked: bool,
    pub last_review: Option<DateTime<Utc>>,
}

/// GET /admin/review-candidates (admin only) -- accounts active in the
/// last day with a flagged signal, and accounts with pending spam, fraud or
/// impersonation reports. Most flags first.
pub async fn list_candidates(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Vec<Candidate>>, AppError> {
    require_admin(&state.db, &auth).await?;
    let mut conn = state.db.acquire().await.db_err("Database error")?;

    let users = sqlx::query_as::<_, (Uuid, String, i64, bool, Option<DateTime<Utc>>)>(
        r#"
        WITH active AS (
            SELECT user_id AS id FROM posts WHERE created_at > NOW() - INTERVAL '1 day'
            UNION SELECT user_id FROM comments WHERE created_at > NOW() - INTERVAL '1 day'
            UNION SELECT sender_id FROM messages WHERE created_at > NOW() - INTERVAL '1 day'
        ),
        spam AS (
            SELECT COALESCE(p.user_id, c.user_id, u0.id) AS id, COUNT(*) AS n
            FROM reports r
            LEFT JOIN posts p ON r.target_type = 'post' AND p.id = r.target_id
            LEFT JOIN comments c ON r.target_type = 'comment' AND c.id = r.target_id
            LEFT JOIN users u0 ON r.target_type = 'user' AND u0.id = r.target_id
            WHERE r.status = 'pending' AND r.reason IN ('spam', 'fraud', 'impersonation')
            GROUP BY 1
        )
        SELECT u.id, u.username, COALESCE(s.n, 0),
               EXISTS (SELECT 1 FROM account_locks l WHERE l.user_id = u.id AND l.unlocked_at IS NULL),
               (SELECT MAX(opened_at) FROM account_reviews ar WHERE ar.user_id = u.id)
        FROM users u
        LEFT JOIN spam s ON s.id = u.id
        WHERE u.id IN (SELECT id FROM active WHERE id IS NOT NULL) OR s.n > 0
        LIMIT 500
        "#,
    )
    .fetch_all(&mut *conn)
    .await
    .db_err("Database error")?;

    let mut out = Vec::new();
    for (id, username, spam_reports, locked, last_review) in users {
        let signals = signals(&mut conn, id).await?;
        let flags = signals.flags();
        if flags.is_empty() && spam_reports == 0 {
            continue;
        }
        out.push(Candidate { username, signals, flags, spam_reports, locked, last_review });
    }
    out.sort_by_key(|c| std::cmp::Reverse((c.flags.len(), c.spam_reports)));
    Ok(Json(out))
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Overview {
    pub username: String,
    pub display_name: Option<String>,
    pub created_at: DateTime<Utc>,
    pub email_verified: bool,
    pub is_private: bool,
    pub post_count: i64,
    pub follower_count: i64,
    pub following_count: i64,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct PostItem {
    pub id: Uuid,
    pub caption: Option<String>,
    pub created_at: DateTime<Utc>,
    pub moderation_status: String,
    pub image_count: i64,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct CommentItem {
    pub id: Uuid,
    pub body: String,
    pub created_at: DateTime<Utc>,
    pub post_id: Uuid,
    pub post_author: Option<String>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct LikeItem {
    pub post_id: Uuid,
    pub post_author: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct FollowItem {
    pub username: String,
    pub created_at: DateTime<Utc>,
}

/// Direct messages, as numbers only.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct MessageStats {
    pub sent_24h: i64,
    pub recipients_24h: i64,
    pub sent_7d: i64,
    pub recipients_7d: i64,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ReportItem {
    pub reason: String,
    pub target_type: String,
    pub status: String,
    pub details: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct HistoryItem {
    /// "decision", "lock" or "review".
    pub kind: String,
    /// The restriction, the lock's end, or the review's outcome.
    pub what: Option<String>,
    pub at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct Review {
    pub review_id: Uuid,
    pub overview: Overview,
    pub signals: Signals,
    pub flags: Vec<&'static str>,
    pub standing_score: i64,
    pub suspended: bool,
    pub locked: bool,
    pub posts: Vec<PostItem>,
    pub comments: Vec<CommentItem>,
    pub likes: Vec<LikeItem>,
    pub follows: Vec<FollowItem>,
    pub messages: MessageStats,
    pub reports: Vec<ReportItem>,
    pub history: Vec<HistoryItem>,
}

#[derive(Debug, Deserialize)]
pub struct OpenRequest {
    /// Why the account is being reviewed. Required.
    pub reason: String,
    pub report_id: Option<Uuid>,
}

/// POST /admin/users/:username/review (admin only) -- opens a review:
/// records who opened it, when and why, and returns the account's recent
/// activity.
pub async fn open_review(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(username): Path<String>,
    Json(input): Json<OpenRequest>,
) -> Result<Json<Review>, AppError> {
    require_admin(&state.db, &auth).await?;
    let reason = checked_text(&input.reason, "A reason")?;

    let mut tx = state.db.begin().await.db_err("Database error")?;
    let overview = sqlx::query_as::<_, Overview>(
        r#"
        SELECT username, display_name, created_at, email_verified, is_private,
               post_count, follower_count, following_count
        FROM users WHERE LOWER(username) = LOWER($1)
        "#,
    )
    .bind(&username)
    .fetch_optional(&mut *tx)
    .await
    .db_err("Database error")?
    .ok_or_else(|| AppError::not_found("User not found"))?;
    let user_id = sqlx::query_scalar::<_, Uuid>("SELECT id FROM users WHERE LOWER(username) = LOWER($1)")
        .bind(&username)
        .fetch_one(&mut *tx)
        .await
        .db_err("Database error")?;

    let review_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO account_reviews (user_id, reviewer_id, reason, report_id) VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(user_id)
    .bind(auth.user_id)
    .bind(&reason)
    .bind(input.report_id)
    .fetch_one(&mut *tx)
    .await
    .db_err_ctx("Failed to record the review", "Database error")?;

    let signals = signals(&mut tx, user_id).await?;
    let flags = signals.flags();

    let posts = sqlx::query_as::<_, PostItem>(
        r#"
        SELECT p.id, p.caption, p.created_at, p.moderation_status::text AS moderation_status,
               (SELECT COUNT(*) FROM media_assets m WHERE m.post_id = p.id) AS image_count
        FROM posts p WHERE p.user_id = $1 ORDER BY p.created_at DESC LIMIT $2
        "#,
    )
    .bind(user_id)
    .bind(LIST_LIMIT)
    .fetch_all(&mut *tx)
    .await
    .db_err("Database error")?;

    let comments = sqlx::query_as::<_, CommentItem>(
        r#"
        SELECT c.id, c.body, c.created_at, c.post_id, pu.username AS post_author
        FROM comments c JOIN posts p ON p.id = c.post_id LEFT JOIN users pu ON pu.id = p.user_id
        WHERE c.user_id = $1 ORDER BY c.created_at DESC LIMIT $2
        "#,
    )
    .bind(user_id)
    .bind(LIST_LIMIT)
    .fetch_all(&mut *tx)
    .await
    .db_err("Database error")?;

    let likes = sqlx::query_as::<_, LikeItem>(
        r#"
        SELECT l.post_id, pu.username AS post_author, l.created_at
        FROM likes l JOIN posts p ON p.id = l.post_id LEFT JOIN users pu ON pu.id = p.user_id
        WHERE l.user_id = $1 ORDER BY l.created_at DESC LIMIT $2
        "#,
    )
    .bind(user_id)
    .bind(LIST_LIMIT)
    .fetch_all(&mut *tx)
    .await
    .db_err("Database error")?;

    let follows = sqlx::query_as::<_, FollowItem>(
        r#"
        SELECT u.username, f.created_at FROM follows f JOIN users u ON u.id = f.following_id
        WHERE f.follower_id = $1 ORDER BY f.created_at DESC LIMIT $2
        "#,
    )
    .bind(user_id)
    .bind(LIST_LIMIT)
    .fetch_all(&mut *tx)
    .await
    .db_err("Database error")?;

    // Counts only: the recipient is the other participant of each
    // conversation the message went to.
    let messages = sqlx::query_as::<_, MessageStats>(
        r#"
        SELECT
            COUNT(*) FILTER (WHERE m.created_at > NOW() - INTERVAL '1 day') AS sent_24h,
            COUNT(DISTINCT c.id) FILTER (WHERE m.created_at > NOW() - INTERVAL '1 day') AS recipients_24h,
            COUNT(*) AS sent_7d,
            COUNT(DISTINCT c.id) AS recipients_7d
        FROM messages m JOIN conversations c ON c.id = m.conversation_id
        WHERE m.sender_id = $1 AND m.created_at > NOW() - INTERVAL '7 days'
        "#,
    )
    .bind(user_id)
    .fetch_one(&mut *tx)
    .await
    .db_err("Database error")?;

    let reports = sqlx::query_as::<_, ReportItem>(
        r#"
        SELECT r.reason::text AS reason, r.target_type::text AS target_type, r.status::text AS status,
               r.details, r.created_at
        FROM reports r
        LEFT JOIN posts p ON r.target_type = 'post' AND p.id = r.target_id
        LEFT JOIN comments c ON r.target_type = 'comment' AND c.id = r.target_id
        WHERE (r.target_type = 'user' AND r.target_id = $1) OR p.user_id = $1 OR c.user_id = $1
        ORDER BY r.created_at DESC LIMIT $2
        "#,
    )
    .bind(user_id)
    .bind(LIST_LIMIT)
    .fetch_all(&mut *tx)
    .await
    .db_err("Database error")?;

    let history = sqlx::query_as::<_, HistoryItem>(
        r#"
        SELECT * FROM (
            SELECT 'decision' AS kind, restriction AS what, created_at AS at
            FROM moderation_decisions WHERE affected_user_id = $1
            UNION ALL
            SELECT 'lock', COALESCE('unlocked by ' || unlocked_via, 'active'), locked_at
            FROM account_locks WHERE user_id = $1
            UNION ALL
            SELECT 'review', COALESCE(outcome, 'open'), opened_at
            FROM account_reviews WHERE user_id = $1 AND id != $2
        ) h ORDER BY at DESC LIMIT 50
        "#,
    )
    .bind(user_id)
    .bind(review_id)
    .fetch_all(&mut *tx)
    .await
    .db_err("Database error")?;

    let (standing_score, suspended, locked) = sqlx::query_as::<_, (i64, bool, bool)>(
        r#"
        SELECT
            LEAST(COALESCE((SELECT SUM(points) FROM account_strikes
                            WHERE user_id = $1 AND (expires_at IS NULL OR expires_at > NOW())), 0), 100)::bigint,
            COALESCE((SELECT suspended_until > NOW() FROM users WHERE id = $1), FALSE),
            EXISTS (SELECT 1 FROM account_locks WHERE user_id = $1 AND unlocked_at IS NULL)
        "#,
    )
    .bind(user_id)
    .fetch_one(&mut *tx)
    .await
    .db_err("Database error")?;

    tx.commit().await.db_err("Database error")?;
    tracing::info!("Account review {} of user {} opened by admin {}", review_id, user_id, auth.user_id);

    Ok(Json(Review {
        review_id,
        overview,
        signals,
        flags,
        standing_score,
        suspended,
        locked,
        posts,
        comments,
        likes,
        follows,
        messages,
        reports,
        history,
    }))
}

#[derive(Debug, Deserialize)]
pub struct DecideRequest {
    /// "no_action", "lock" (suspected takeover) or "bot" (bot or spam-only
    /// account: permanent suspension).
    pub outcome: String,
    /// Required; for "lock" it is the lock's reason, for "bot" it stays
    /// internal like any review note.
    pub note: String,
}

/// The statement basis for a ban after a review found a bot.
const BOT_BASIS: &str = "Nach Prüfung deines Kontos gehen wir davon aus, dass es automatisiert betrieben oder nur \
    für Spam angelegt wurde. Das verbieten unsere Nutzungsbedingungen in Abschnitt 4 (Spam, automatisierte \
    Massen-Registrierungen oder Bots).";

/// POST /admin/reviews/:id/decide (admin only) -- closes a review with its
/// outcome and carries it out.
pub async fn decide_review(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(review_id): Path<Uuid>,
    Json(input): Json<DecideRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_admin(&state.db, &auth).await?;
    let note = checked_text(&input.note, "A note")?;
    let outcome = match input.outcome.as_str() {
        "no_action" => "no_action",
        "lock" => "locked",
        "bot" => "bot",
        _ => return Err(AppError::bad_request("Invalid outcome")),
    };

    let user_id = sqlx::query_scalar::<_, Option<Uuid>>(
        "SELECT user_id FROM account_reviews WHERE id = $1 AND outcome IS NULL",
    )
    .bind(review_id)
    .fetch_optional(&state.db)
    .await
    .db_err("Database error")?
    .ok_or_else(|| AppError::conflict("This review is already decided"))?
    .ok_or_else(|| AppError::conflict("The account no longer exists"))?;

    // Carried out first: if the lock or ban fails, the review stays open.
    match outcome {
        "locked" => lock_user(&state, user_id, auth.user_id, &note).await?,
        "bot" => {
            if user_id == auth.user_id {
                return Err(AppError::bad_request("You can't take a measure against your own account"));
            }
            let mut tx = state.db.begin().await.db_err("Database error")?;
            let (_, notices) =
                moderation::record_account_measure(&mut tx, user_id, Measure::Ban, "spam", auth.user_id, 0, Some(BOT_BASIS))
                    .await?;
            tx.commit().await.db_err("Database error")?;
            notices.send(&state).await;
        }
        _ => {}
    }

    sqlx::query(
        "UPDATE account_reviews SET outcome = $2, outcome_note = $3, decided_at = NOW() WHERE id = $1 AND outcome IS NULL",
    )
    .bind(review_id)
    .bind(outcome)
    .bind(&note)
    .execute(&state.db)
    .await
    .db_err("Database error")?;

    tracing::info!("Account review {} decided by admin {}: {}", review_id, auth.user_id, outcome);
    Ok(Json(serde_json::json!({ "outcome": outcome })))
}
