//! Notices about illegal content from anyone, with or without an account
//! (DSA Art. 16(1)): the public form on the website. A notice names the
//! content by its link, says why it is illegal and, unless it is about
//! child sexual abuse material, who sends it (Art. 16(2)). It becomes a
//! report with source "public_notice" in the normal queue -- no automatic
//! hide, since nothing is known about the sender -- and the notifier follows
//! it through a status link and hears the outcome by email (Art. 16(4) and
//! (5); moderation::close_reports). Rights claims (copyright and the like)
//! have a form of their own (rights.rs).

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::alerts::{self, Alert};
use crate::auth::{generate_refresh_token, hash_refresh_token, OptionalAuthUser};
use crate::errors::AppError;
use crate::evidence;
use crate::handlers::auth::AppState;
use crate::handlers::reports::VALID_REASONS;
use crate::moderation::PendingNotices;
use crate::utils::DbResultExt;
use crate::validation::{required_text, validate_new_email};

const NAME_MAX: usize = 200;
const URL_MAX: usize = 500;
const TEXT_MAX: usize = 4000;

/// What a link on Klar points to. Posts look like `…/posts/<id>`, a
/// comment adds `#comment-<id>` (the comment's own link in the app), a
/// profile is `…/users/<name>`. The host isn't checked: the item has to
/// exist on Klar either way.
#[derive(Debug, PartialEq)]
enum Linked {
    Post(Uuid),
    Comment { post: Uuid, comment: Uuid },
    Profile(String),
}

fn uuid_at_start(text: &str) -> Option<Uuid> {
    let id: String = text.chars().take_while(|c| c.is_ascii_hexdigit() || *c == '-').collect();
    Uuid::parse_str(&id).ok()
}

fn parse_link(url: &str) -> Option<Linked> {
    if let Some(rest) = url.split("/posts/").nth(1) {
        let post = uuid_at_start(rest)?;
        let comment = rest
            .split("comment-")
            .nth(1)
            .or_else(|| rest.split("comment=").nth(1))
            .and_then(uuid_at_start);
        return Some(match comment {
            Some(comment) => Linked::Comment { post, comment },
            None => Linked::Post(post),
        });
    }
    let rest = url.split("/users/").nth(1)?;
    let name: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '.').collect();
    (!name.is_empty()).then_some(Linked::Profile(name))
}

#[derive(Debug, Deserialize)]
pub struct CreateNoticeRequest {
    /// One of the report reasons.
    pub reason: String,
    /// Why the content is illegal (Art. 16(2)(a)).
    pub explanation: String,
    /// Where it is (Art. 16(2)(b)).
    pub content_url: String,
    /// Required, except for child sexual abuse material (Art. 16(2)(c)).
    pub notifier_name: Option<String>,
    pub notifier_email: Option<String>,
    /// The statement that the notice is accurate and complete to the best
    /// of the notifier's knowledge (Art. 16(2)(d)).
    pub good_faith: bool,
    /// Honeypot: hidden in the form, so only bots fill it in.
    #[serde(default)]
    pub website: String,
}

#[derive(Debug, Serialize)]
pub struct CreateNoticeResponse {
    pub id: Uuid,
    /// The notifier's key to the status page; also sent by email when an
    /// address was given. Only its hash is stored.
    pub token: String,
}

/// POST /notices (public, strict rate limit)
pub async fn create_notice(
    State(state): State<AppState>,
    auth: OptionalAuthUser,
    Json(input): Json<CreateNoticeRequest>,
) -> Result<(StatusCode, Json<CreateNoticeResponse>), AppError> {
    if !input.website.is_empty() {
        // Looks accepted to the bot; nothing is stored.
        tracing::info!("Notice form honeypot triggered");
        return Ok((StatusCode::CREATED, Json(CreateNoticeResponse { id: Uuid::nil(), token: String::new() })));
    }
    if !VALID_REASONS.contains(&input.reason.as_str()) {
        return Err(AppError::bad_request("Invalid reason"));
    }
    if !input.good_faith {
        return Err(AppError::bad_request("The good-faith statement is required"));
    }
    let explanation = required_text(&input.explanation, "Explanation", TEXT_MAX)?;
    let content_url = required_text(&input.content_url, "Link", URL_MAX)?;
    let name = input.notifier_name.as_deref().map(str::trim).filter(|n| !n.is_empty());
    if name.is_some_and(|n| n.chars().count() > NAME_MAX) {
        return Err(AppError::bad_request(format!("Name must be {} characters or less", NAME_MAX)));
    }
    let email = match input.notifier_email.as_deref().map(str::trim).filter(|e| !e.is_empty()) {
        Some(email) => Some(validate_new_email(email)?),
        None => None,
    };
    if input.reason != "csam" && (name.is_none() || email.is_none()) {
        return Err(AppError::bad_request("Your name and email address are required"));
    }

    let mut tx = state.db.begin().await.db_err("Database error")?;
    let not_found = || AppError::not_found("We couldn't find that content on Klar (any more)");
    let (target_type, target_id) = match parse_link(content_url) {
        Some(Linked::Post(post)) => {
            let exists = sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM posts WHERE id = $1 AND moderation_status != 'removed')",
            )
            .bind(post)
            .fetch_one(&mut *tx)
            .await
            .db_err("Database error")?;
            if !exists {
                return Err(not_found());
            }
            ("post", post)
        }
        Some(Linked::Comment { post, comment }) => {
            let exists = sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM comments WHERE id = $1 AND post_id = $2 AND moderation_status != 'removed')",
            )
            .bind(comment)
            .bind(post)
            .fetch_one(&mut *tx)
            .await
            .db_err("Database error")?;
            if !exists {
                return Err(not_found());
            }
            ("comment", comment)
        }
        Some(Linked::Profile(username)) => {
            let user = sqlx::query_scalar::<_, Uuid>("SELECT id FROM users WHERE LOWER(username) = LOWER($1)")
                .bind(&username)
                .fetch_optional(&mut *tx)
                .await
                .db_err("Database error")?
                .ok_or_else(not_found)?;
            ("user", user)
        }
        None => {
            return Err(AppError::bad_request(
                "The link must point to a post, a comment or a profile on Klar (…/posts/…, …/users/…)",
            ))
        }
    };

    let token = generate_refresh_token();
    let notice_id = sqlx::query_scalar::<_, Uuid>(
        r#"
        INSERT INTO content_notices
            (reason, explanation, content_url, target_type, target_id, notifier_name, notifier_email,
             notifier_user_id, good_faith, status_token_hash)
        VALUES ($1::report_reason, $2, $3, $4::report_target_type, $5, $6, $7, $8, TRUE, $9)
        RETURNING id
        "#,
    )
    .bind(&input.reason)
    .bind(explanation)
    .bind(content_url)
    .bind(target_type)
    .bind(target_id)
    .bind(name)
    .bind(&email)
    .bind(auth.user_id)
    .bind(hash_refresh_token(&token))
    .fetch_one(&mut *tx)
    .await
    .db_err_ctx("Failed to store notice", "Database error")?;

    // The report the queue works with. No reporter account: the notifier
    // is known through the notice, and hears the outcome by email.
    sqlx::query(
        r#"
        INSERT INTO reports (reporter_id, target_type, target_id, reason, details, source, notice_id)
        VALUES (NULL, $1::report_target_type, $2, $3::report_reason, $4, 'public_notice', $5)
        "#,
    )
    .bind(target_type)
    .bind(target_id)
    .bind(&input.reason)
    .bind(explanation)
    .bind(notice_id)
    .execute(&mut *tx)
    .await
    .db_err_ctx("Failed to store notice", "Database error")?;

    let preserved = if evidence::preserves(target_type, &input.reason) {
        evidence::capture(&mut tx, target_type, target_id, evidence::Cause::Reported, None).await?
    } else {
        evidence::Preserved::default()
    };
    tx.commit().await.db_err("Database error")?;

    let mut notices = PendingNotices::default();
    if alerts::is_urgent(&input.reason) {
        notices.alert(Alert::UrgentReport {
            target_type: target_type.to_string(),
            target_id,
            reason: input.reason.clone(),
        });
    }
    notices.send(&state).await;
    evidence::finish(&state, preserved, Vec::new()).await;

    // Confirmation of receipt (Art. 16(4)) with the status link.
    if let Some(email) = email {
        let state = state.clone();
        let token = token.clone();
        tokio::spawn(async move {
            if let Err(e) = state.email.send_notice_received(&email, notice_id, &token).await {
                tracing::error!("Notice {} confirmation email failed: {}", notice_id, e.0);
            }
        });
    }

    tracing::info!("Notice {} received ({}, {} {})", notice_id, input.reason, target_type, target_id);
    Ok((StatusCode::CREATED, Json(CreateNoticeResponse { id: notice_id, token })))
}

#[derive(Debug, Deserialize)]
pub struct NoticeTokenRequest {
    pub token: String,
}

/// What the notifier sees on their status page.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct NoticeStatus {
    pub id: Uuid,
    pub reason: String,
    pub content_url: String,
    pub explanation: String,
    pub created_at: DateTime<Utc>,
    pub decided_at: Option<DateTime<Utc>>,
    /// "removed", "account_measure", "no_violation", "obsolete"; None while
    /// it is being reviewed.
    pub outcome: Option<String>,
}

/// POST /notices/:id/status (public; the token is the key). POST so the
/// token travels in the body, not in a URL that ends up in logs.
pub async fn notice_status(
    State(state): State<AppState>,
    Path(notice_id): Path<Uuid>,
    Json(input): Json<NoticeTokenRequest>,
) -> Result<Json<NoticeStatus>, AppError> {
    sqlx::query_as::<_, NoticeStatus>(
        r#"
        SELECT id, reason::text AS reason, content_url, explanation, created_at, decided_at, outcome
        FROM content_notices WHERE id = $1 AND status_token_hash = $2
        "#,
    )
    .bind(notice_id)
    .bind(hash_refresh_token(input.token.trim()))
    .fetch_optional(&state.db)
    .await
    .db_err("Database error")?
    .map(Json)
    // Same answer for "no such notice" and "wrong token".
    .ok_or_else(|| AppError::not_found("Notice not found"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_name_posts_comments_and_profiles() {
        let (post, comment) = (Uuid::new_v4(), Uuid::new_v4());
        assert_eq!(parse_link(&format!("https://www.klarsocial.eu/posts/{post}")), Some(Linked::Post(post)));
        assert_eq!(
            parse_link(&format!("https://klarsocial.de/posts/{post}#comment-{comment}")),
            Some(Linked::Comment { post, comment })
        );
        assert_eq!(parse_link(&format!("/posts/{post}?comment={comment}")), Some(Linked::Comment { post, comment }));
        assert_eq!(parse_link("https://www.klarsocial.eu/users/some.one_1?tab=posts"), Some(Linked::Profile("some.one_1".into())));
        assert_eq!(parse_link("https://www.klarsocial.eu/feed"), None);
        assert_eq!(parse_link("https://www.klarsocial.eu/posts/nope"), None);
    }
}
