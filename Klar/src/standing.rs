//! Account standing: strike points for upheld violations, the score they
//! add up to, and the account measures (warning, suspension, permanent
//! suspension) the score suggests.
//!
//! An admin removing reported content classifies it as one of the violation
//! types in VIOLATIONS. Each type has a written criterion and fixed points,
//! so the admin decides which described case the content matches, not how
//! many points it deserves -- that leaves far less room for bias than
//! picking a severity freely. The report reason alone can't decide it: the
//! reporter picks it (and could pick "terrorism" for an insult), and one
//! reason covers very different things ("harassment" is both a single "you
//! suck" and stalking). An admin can classify content under another reason
//! than the reporter's, or remove it without a strike, but only with a
//! written justification, stored with the decision.
//!
//! Strikes expire (and are deleted, with their snapshot of the removed
//! content) after a period that grows with the severity, so a few old minor
//! insults never add up to a suspension, and we don't keep the record longer
//! than it is useful. Repeating the same kind of violation in a short time
//! is more likely deliberate trolling, so from the third strike for the same
//! reason within 30 days, the points count one and a half times.
//!
//! The score never acts on its own: it suggests the next measure, and an
//! admin decides (see the migration for why). Suspended accounts are
//! read-only and hidden from everyone else; `enforce_suspension` blocks
//! their writes, and the read paths filter them out with
//! `suspended_until IS NULL OR suspended_until <= NOW()` (spelled out: a
//! plain `NOT suspended_until > NOW()` is NULL, i.e. false, for accounts
//! never suspended).

use axum::{
    extract::{MatchedPath, Request, State},
    http::Method,
    middleware::Next,
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::auth::OptionalAuthUser;
use crate::errors::AppError;
use crate::handlers::auth::AppState;
use crate::utils::DbResultExt;

/// The score is capped here; reaching it suggests a permanent suspension.
pub const MAX_SCORE: i32 = 100;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Content removed, but no strike (e.g. a post about someone's own
    /// crisis, removed to protect them, not as a sanction).
    None,
    Minor,
    Moderate,
    Serious,
    /// Hard, but not an instant ban: a first case suggests a warning (the
    /// warning-first rule), a second within the year a permanent ban.
    Grave,
    Severe,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::None => "none",
            Severity::Minor => "minor",
            Severity::Moderate => "moderate",
            Severity::Serious => "serious",
            Severity::Grave => "grave",
            Severity::Severe => "severe",
        }
    }

    pub fn points(self) -> i32 {
        match self {
            Severity::None => 0,
            Severity::Minor => 5,
            Severity::Moderate => 20,
            Severity::Serious => 40,
            Severity::Grave => 60,
            Severity::Severe => MAX_SCORE,
        }
    }

    /// How long a strike counts. None: it doesn't expire.
    pub fn lifetime_days(self) -> Option<i32> {
        match self {
            Severity::None => Some(0), // never stored, see add_strike
            Severity::Minor => Some(90),
            Severity::Moderate => Some(180),
            Severity::Serious | Severity::Grave => Some(365),
            Severity::Severe => None,
        }
    }
}

/// Whether removed content of a violation type goes to the authorities.
/// ⚖️ Which types, pending review.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AuthorityReport {
    None,
    /// A suspected crime that may endanger the rest of the platform or the
    /// public (e.g. Holocaust denial, § 130(3) StGB): the admin decides.
    Recommended,
    /// A suspected crime threatening someone's life or safety: providers
    /// must tell the authorities (DSA Art. 18).
    Required,
}

impl AuthorityReport {
    pub fn as_db(self) -> Option<&'static str> {
        match self {
            AuthorityReport::None => None,
            AuthorityReport::Recommended => Some("recommended"),
            AuthorityReport::Required => Some("required"),
        }
    }
}

/// One entry of the violation catalog. The German label and criterion are
/// shown in the statement of reasons and in Terms of Service section 8;
/// keep the three in sync. ⚖️ Catalog pending review.
#[derive(Debug, Serialize)]
pub struct Violation {
    pub id: &'static str,
    /// The report reason it belongs to.
    pub reason: &'static str,
    pub severity: Severity,
    pub label: &'static str,
    pub label_de: &'static str,
    pub criterion_de: &'static str,
    pub authority_report: AuthorityReport,
}

const fn v(
    id: &'static str,
    reason: &'static str,
    severity: Severity,
    label: &'static str,
    label_de: &'static str,
    criterion_de: &'static str,
) -> Violation {
    Violation { id, reason, severity, label, label_de, criterion_de, authority_report: AuthorityReport::None }
}

impl Violation {
    const fn report(mut self, authority_report: AuthorityReport) -> Self {
        self.authority_report = authority_report;
        self
    }
}

/// The catalog, in report-reason order. The first type of a reason is the
/// default for a report with that reason.
pub const VIOLATIONS: &[Violation] = &[
    v("spam_single", "spam", Severity::Minor, "Spam", "Spam",
      "Einzelne unerwünschte Werbung oder sinnlose Wiederholungen."),
    v("spam_mass", "spam", Severity::Moderate, "Mass spam", "Massenspam",
      "Viele gleichartige Inhalte oder Nachrichten in kurzer Zeit von einem sonst normal genutzten Konto."),
    // Severe, so the admin page suggests a permanent suspension at once. Not
    // for a hijacked account spamming: its owner is the victim (no strike,
    // with a justification, and a password reset instead).
    v("bot_account", "spam", Severity::Severe, "Bot or spam-only account", "Bot- oder reines Spam-Konto",
      "Das Konto wird automatisiert betrieben oder wurde nur für Spam oder Massen-Registrierungen angelegt. \
       Nicht gemeint sind Konten, die von Dritten übernommen wurden."),
    v("insult", "harassment", Severity::Minor, "Single insult", "Einzelne Beleidigung",
      "Eine einzelne herabsetzende Äußerung gegenüber einer Person, ohne Drohung und ohne erkennbares Muster."),
    v("harassment_targeted", "harassment", Severity::Moderate, "Targeted or repeated harassment",
      "Gezielte oder wiederholte Belästigung",
      "Eine Person wird wiederholt oder gezielt herabgesetzt, bloßgestellt oder gegen ihren Willen bedrängt."),
    v("threat_stalking", "harassment", Severity::Serious, "Threats, stalking or doxxing",
      "Drohung, Stalking oder Veröffentlichung privater Daten",
      "Androhung von Gewalt oder anderen Übeln gegen eine Person, beharrliches Nachstellen oder Veröffentlichen \
       privater Daten wie Adresse oder Telefonnummer.").report(AuthorityReport::Recommended),
    v("hate_derogatory", "hate_speech", Severity::Moderate, "Derogatory generalisation about a group",
      "Abwertende Verallgemeinerung über eine Gruppe",
      "Herabsetzende Aussage über Menschen wegen eines in Abschnitt 4 genannten Merkmals, ohne Entmenschlichung \
       und ohne Aufruf."),
    v("hate_dehumanising", "hate_speech", Severity::Serious, "Dehumanising or inciting against a group",
      "Entmenschlichung oder Hetze gegen eine Gruppe",
      "Menschen wird wegen eines in Abschnitt 4 genannten Merkmals das Menschsein abgesprochen, etwa durch \
       Gleichsetzung mit Tieren oder Ungeziefer, oder es wird zu Hass, Gewalt oder Ausgrenzung gegen sie aufgerufen.")
      .report(AuthorityReport::Recommended),
    v("extremism_glorifying", "extremism", Severity::Grave,
      "Glorifying Nazism/fascism, illegal symbols", "Verherrlichung von NS/Faschismus, verbotene Symbole",
      "Verherrlichung, Verharmlosung oder Rechtfertigung des Nationalsozialismus, des Faschismus oder ihrer \
       Verbrechen, oder Verwenden von Kennzeichen verbotener oder verfassungswidriger Organisationen \
       (§ 86a StGB) oder extremistischer Parolen, ohne Werbung für eine Organisation."),
    // Severe: recruiting for an extremist organisation and Holocaust denial
    // (§ 130(3) StGB) suggest a permanent suspension on the first case.
    v("extremism_promotion", "extremism", Severity::Severe,
      "Promoting an extremist organisation, Holocaust denial",
      "Werbung für eine extremistische Organisation, Holocaustleugnung",
      "Werbung für, Unterstützung von oder Anwerbung für eine extremistische Organisation im Sinne von \
       Abschnitt 4, oder Leugnung des Holocaust.").report(AuthorityReport::Recommended),
    v("violence_graphic", "violence", Severity::Moderate, "Graphic violence without context",
      "Drastische Gewaltdarstellung ohne Einordnung",
      "Verstörende Darstellung von Gewalt oder Verletzungen ohne dokumentarischen oder aufklärenden Zusammenhang."),
    v("violence_glorifying", "violence", Severity::Serious, "Glorifying violence", "Verherrlichung von Gewalt",
      "Gewalt gegen Menschen oder Tiere wird gefeiert, verherrlicht oder als nachahmenswert dargestellt."),
    // Severe, like a concrete threat: terrorist content is a crime (§§ 89a,
    // 91, 129a StGB) and has to go within an hour of a removal order (EU
    // Regulation 2021/784). The criterion is promoting, not reporting on it.
    v("terror_propaganda", "terrorism", Severity::Severe, "Terrorist propaganda", "Terroristische Propaganda",
      "Inhalte terroristischer Organisationen oder deren Verherrlichung, ohne konkrete Drohung.")
      .report(AuthorityReport::Recommended),
    v("terror_threat", "terrorism", Severity::Severe, "Concrete threat of an attack or serious violence",
      "Konkrete Androhung eines Anschlags oder schwerer Gewalt",
      "Ankündigung oder ernst gemeinte Drohung mit einer schweren Gewalttat, etwa einem Anschlag oder Amoklauf.")
      .report(AuthorityReport::Required),
    v("self_harm_crisis", "self_harm", Severity::None, "Own crisis (no strike)", "Eigene Krise (keine Punkte)",
      "Die Person spricht über eigene Selbstverletzung oder Suizidgedanken. Der Inhalt wird zu ihrem Schutz \
       entfernt, ohne Punkte."),
    v("self_harm_encouraging", "self_harm", Severity::Serious, "Encouraging or instructing self-harm",
      "Aufforderung oder Anleitung zu Selbstverletzung",
      "Andere werden zu Selbstverletzung oder Suizid ermutigt oder dazu angeleitet."),
    v("sexual_explicit", "sexual_content", Severity::Moderate, "Sexually explicit content",
      "Sexuell expliziter Inhalt",
      "Pornografische oder sexuell explizite Darstellung. Nicht gemeint ist Nacktheit in Kunst, Aufklärung \
       oder beim Stillen."),
    // Grave: targeted sexual harassment drives exactly the people Klar wants
    // to protect off the platform. A warning first, a ban on the second case.
    v("sexual_harassment", "sexual_content", Severity::Grave, "Sexual harassment", "Sexuelle Belästigung",
      "Unerwünschte sexuelle Äußerungen oder Inhalte, die sich gegen eine bestimmte Person richten."),
    v("ncii_shared", "ncii", Severity::Serious, "Intimate images without consent",
      "Intime Aufnahmen ohne Einwilligung",
      "Verbreitung intimer oder sexueller Aufnahmen einer Person ohne deren Einwilligung."),
    v("ncii_sextortion", "ncii", Severity::Severe, "Sextortion", "Erpressung mit intimen Aufnahmen",
      "Drohung, intime Aufnahmen einer Person zu verbreiten, um von ihr etwas zu erzwingen.")
      .report(AuthorityReport::Recommended),
    v("csam", "csam", Severity::Severe, "Child sexual abuse material",
      "Darstellung sexuellen Missbrauchs von Minderjährigen",
      "Darstellung sexuellen Missbrauchs von Kindern oder Jugendlichen.").report(AuthorityReport::Required),
    v("fraud_scam", "fraud", Severity::Moderate, "Scam attempt", "Betrugsversuch",
      "Versuch, andere durch Täuschung zu Zahlungen oder anderen Leistungen zu bewegen."),
    v("fraud_phishing", "fraud", Severity::Serious, "Phishing", "Phishing",
      "Versuch, Zugangs-, Zahlungs- oder Ausweisdaten anderer zu erschleichen."),
    v("illegal_goods", "illegal_goods", Severity::Serious, "Offering illegal goods",
      "Angebot illegaler Waren", "Angebot von oder Handel mit Drogen, Waffen oder anderen illegalen Waren."),
    v("impersonation_unlabelled", "impersonation", Severity::Minor, "Unlabelled parody",
      "Nicht gekennzeichnete Parodie",
      "Ein Konto gibt sich als andere Person aus, ohne erkennbare Täuschungsabsicht, aber ohne Kennzeichnung \
       als Parodie."),
    v("impersonation_deceptive", "impersonation", Severity::Moderate, "Impersonating someone to deceive",
      "Täuschender Identitätsdiebstahl",
      "Ein Konto gibt sich als reale Person oder Organisation aus, um andere zu täuschen."),
    v("other", "other", Severity::Minor, "Other violation of the Terms", "Sonstiger Verstoß",
      "Ein anderer Verstoß gegen die Nutzungsbedingungen, der zu keiner anderen Art passt."),
];

pub fn violation(id: &str) -> Option<&'static Violation> {
    VIOLATIONS.iter().find(|v| v.id == id)
}

/// The default classification for a report reason (its first type).
pub fn default_violation(reason: &str) -> Option<&'static Violation> {
    VIOLATIONS.iter().find(|v| v.reason == reason)
}

/// How an admin classified a removal: a violation type, or no strike.
/// `note` is the justification, required for a type from another reason
/// than the reporter's and for no strike (see `classify`).
pub struct Classification {
    pub violation: Option<&'static Violation>,
    pub note: Option<String>,
}

impl Classification {
    pub fn id(&self) -> &'static str {
        self.violation.map(|v| v.id).unwrap_or("none")
    }
}

const NOTE_MAX: usize = 1000;

/// Validates an admin's classification of a removal after a report with
/// `reported_reason`. `violation`: a catalog id, "none", or nothing for the
/// reason's default.
pub fn classify(reported_reason: &str, violation: Option<&str>, note: Option<&str>) -> Result<Classification, AppError> {
    let note = note.map(str::trim).filter(|n| !n.is_empty());
    if note.is_some_and(|n| n.chars().count() > NOTE_MAX) {
        return Err(AppError::bad_request(format!("Justification must be under {} characters", NOTE_MAX)));
    }
    let chosen = match violation {
        None => default_violation(reported_reason),
        Some("none") => None,
        Some(id) => Some(self::violation(id).ok_or_else(|| AppError::bad_request("Unknown violation type"))?),
    };
    let needs_note = match chosen {
        None => violation.is_some(),
        Some(v) => v.reason != reported_reason,
    };
    if needs_note && note.is_none() {
        return Err(AppError::bad_request(
            "A justification is required when classifying under another reason than the report's, or giving no strike",
        ));
    }
    Ok(Classification { violation: chosen, note: note.map(String::from) })
}

/// Repeat offences: the third and every further strike for the same report
/// reason within REPEAT_WINDOW_DAYS counts one and a half times (rounded up).
pub const REPEAT_WINDOW_DAYS: i32 = 30;
pub const REPEAT_FROM: i64 = 3;

fn with_repeat_factor(base: i32) -> i32 {
    // Rounded up (i32::div_ceil is still unstable for signed integers).
    ((base * 3 + 1) / 2).min(MAX_SCORE)
}

/// The points a new strike for `v` gets for this user, with the repeat
/// factor applied. Read before the strike is stored, so the statement can
/// state them.
pub async fn strike_points(tx: &mut PgConnection, user_id: Uuid, v: &Violation) -> Result<(i32, i32), AppError> {
    let base = v.severity.points();
    let earlier = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*) FROM account_strikes
        WHERE user_id = $1 AND reason = $2::report_reason AND created_at > NOW() - make_interval(days => $3)
        "#,
    )
    .bind(user_id)
    .bind(v.reason)
    .bind(REPEAT_WINDOW_DAYS)
    .fetch_one(&mut *tx)
    .await
    .db_err("Database error")?;
    let points = if earlier + 1 >= REPEAT_FROM { with_repeat_factor(base) } else { base };
    Ok((base, points))
}

/// SELECT producing the snapshot of the post or comment with id $1: the
/// removed content itself (the only part that is the author's own data, so
/// the only part in their export), its context, and the reports on it --
/// reason, description and time, not who reported. Images aren't copied;
/// likely-illegal ones are preserved in the evidence store.
fn snapshot_sql(target_type: &str) -> &'static str {
    match target_type {
        "post" => r#"
            SELECT jsonb_build_object(
                'content', jsonb_build_object('type', 'post', 'text', p.caption, 'created_at', p.created_at,
                    'edited_at', p.edited_at,
                    'image_count', (SELECT COUNT(*) FROM media_assets m WHERE m.post_id = p.id)),
                'context', NULL,
                'reports', COALESCE((SELECT jsonb_agg(jsonb_build_object('reason', r.reason, 'details', r.details,
                    'created_at', r.created_at) ORDER BY r.created_at)
                    FROM reports r WHERE r.target_type = 'post' AND r.target_id = p.id), '[]'::jsonb),
                'removed_at', NOW())
            FROM posts p WHERE p.id = $1
            "#,
        _ => r#"
            SELECT jsonb_build_object(
                'content', jsonb_build_object('type', 'comment', 'text', c.body, 'created_at', c.created_at,
                    'edited_at', c.edited_at),
                'context', jsonb_build_object(
                    'post', (SELECT jsonb_build_object('id', p.id, 'text', p.caption, 'author', pu.username,
                        'created_at', p.created_at) FROM posts p JOIN users pu ON pu.id = p.user_id
                        WHERE p.id = c.post_id),
                    'parent_comment', (SELECT jsonb_build_object('text', pc.body, 'author', pcu.username,
                        'created_at', pc.created_at) FROM comments pc JOIN users pcu ON pcu.id = pc.user_id
                        WHERE pc.id = c.parent_comment_id)),
                'reports', COALESCE((SELECT jsonb_agg(jsonb_build_object('reason', r.reason, 'details', r.details,
                    'created_at', r.created_at) ORDER BY r.created_at)
                    FROM reports r WHERE r.target_type = 'comment' AND r.target_id = c.id), '[]'::jsonb),
                'removed_at', NOW())
            FROM comments c WHERE c.id = $1
            "#,
    }
}

pub struct NewStrike<'a> {
    pub user_id: Uuid,
    pub decision_id: Uuid,
    pub violation: &'a Violation,
    pub base_points: i32,
    pub points: i32,
    pub target_type: &'a str,
    pub target_id: Uuid,
}

/// Stores a strike with its snapshot. Runs in the removal's transaction,
/// before the content is deleted.
pub async fn add_strike(tx: &mut PgConnection, s: NewStrike<'_>) -> Result<(), AppError> {
    if s.base_points == 0 {
        return Ok(());
    }
    let snapshot = sqlx::query_scalar::<_, serde_json::Value>(snapshot_sql(s.target_type))
        .bind(s.target_id)
        .fetch_optional(&mut *tx)
        .await
        .db_err_ctx("Failed to snapshot removed content", "Database error")?
        .unwrap_or_else(|| serde_json::json!({ "content": null }));

    sqlx::query(
        r#"
        INSERT INTO account_strikes (user_id, decision_id, violation, reason, severity, base_points, points, snapshot, expires_at)
        VALUES ($1, $2, $3, $4::report_reason, $5, $6, $7, $8,
                CASE WHEN $9::int IS NULL THEN NULL ELSE NOW() + make_interval(days => $9) END)
        ON CONFLICT (decision_id) DO NOTHING
        "#,
    )
    .bind(s.user_id)
    .bind(s.decision_id)
    .bind(s.violation.id)
    .bind(s.violation.reason)
    .bind(s.violation.severity.as_str())
    .bind(s.base_points)
    .bind(s.points)
    .bind(snapshot)
    .bind(s.violation.severity.lifetime_days())
    .execute(&mut *tx)
    .await
    .db_err_ctx("Failed to record strike", "Database error")?;
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Measure {
    Warning,
    Suspend7,
    Suspend30,
    Ban,
}

impl Measure {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "warning" => Some(Measure::Warning),
            "suspend_7d" => Some(Measure::Suspend7),
            "suspend_30d" => Some(Measure::Suspend30),
            "ban" => Some(Measure::Ban),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Measure::Warning => "warning",
            Measure::Suspend7 => "suspend_7d",
            Measure::Suspend30 => "suspend_30d",
            Measure::Ban => "ban",
        }
    }

    /// The `restriction` of its decision record.
    pub fn restriction(self) -> &'static str {
        match self {
            Measure::Warning => "warning",
            Measure::Suspend7 | Measure::Suspend30 => "suspended",
            Measure::Ban => "banned",
        }
    }

    pub fn suspension_days(self) -> Option<i32> {
        match self {
            Measure::Suspend7 => Some(7),
            Measure::Suspend30 => Some(30),
            Measure::Warning | Measure::Ban => None,
        }
    }

    /// The measure a score reaches, before the warning rule in `suggest`.
    pub fn for_score(score: i32) -> Option<Self> {
        match score {
            s if s >= MAX_SCORE => Some(Measure::Ban),
            s if s >= 75 => Some(Measure::Suspend30),
            s if s >= 50 => Some(Measure::Suspend7),
            s if s >= 25 => Some(Measure::Warning),
            _ => None,
        }
    }
}

/// The thresholds, for showing the scale to users and admins.
pub const THRESHOLDS: [(i32, Measure); 4] = [
    (25, Measure::Warning),
    (50, Measure::Suspend7),
    (75, Measure::Suspend30),
    (MAX_SCORE, Measure::Ban),
];

/// The next measure to suggest: the one the score reaches, unless the
/// account already got a measure after its latest strike (nothing new has
/// happened since). A suspension is only suggested once the account has an
/// active warning from the last year, so people are warned before they're
/// suspended (DSA Art. 23(1)) -- except at the maximum score, which only a
/// severe violation reaches.
pub fn suggest(score: i32, strikes_since_last_measure: bool, recently_warned: bool) -> Option<Measure> {
    let reached = Measure::for_score(score)?;
    if !strikes_since_last_measure {
        return None;
    }
    match reached {
        Measure::Suspend7 | Measure::Suspend30 if !recently_warned => Some(Measure::Warning),
        m => Some(m),
    }
}

/// Removes the strike of a decision (its objection was accepted).
pub async fn revoke_strike(tx: &mut PgConnection, decision_id: Uuid) -> Result<(), AppError> {
    sqlx::query("DELETE FROM account_strikes WHERE decision_id = $1")
        .bind(decision_id)
        .execute(&mut *tx)
        .await
        .db_err("Database error")?;
    Ok(())
}

/// A user's current suspension, if any.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Suspension {
    /// None for a permanent suspension.
    pub until: Option<DateTime<Utc>>,
    pub permanent: bool,
    /// For a permanent suspension: when the account is deleted (see
    /// sweep_bans). None while an objection is pending, since deletion
    /// waits for its outcome.
    pub deletion_at: Option<DateTime<Utc>>,
}

/// 'infinity' can't be decoded into a DateTime, so it's mapped to
/// `permanent` in SQL.
pub async fn suspension(conn: &mut PgConnection, user_id: Uuid) -> Result<Option<Suspension>, AppError> {
    let row = sqlx::query_as::<_, (Option<DateTime<Utc>>, bool, Option<DateTime<Utc>>)>(
        r#"
        SELECT CASE WHEN u.suspended_until = 'infinity' THEN NULL ELSE u.suspended_until END,
               u.suspended_until = 'infinity',
               CASE WHEN u.suspended_until = 'infinity' AND d.restriction = 'banned'
                         AND d.objection_status IS DISTINCT FROM 'pending'
                    THEN d.created_at + make_interval(days => $2) END
        FROM users u LEFT JOIN moderation_decisions d ON d.id = u.suspension_decision_id
        WHERE u.id = $1 AND u.suspended_until > NOW()
        "#,
    )
    .bind(user_id)
    .bind(BAN_DELETION_DAYS)
    .fetch_optional(&mut *conn)
    .await
    .db_err("Database error")?;
    Ok(row.map(|(until, permanent, deletion_at)| Suspension { until, permanent, deletion_at }))
}

/// A permanently suspended account is deleted when the objection window
/// (DSA Art. 20(1): at least six months) has passed: keeping a banned
/// account's data with no purpose left would break storage limitation
/// (Art. 5(1)(e) GDPR). Not while an objection is pending, and not while
/// the statement is still held back (CSAM), so nobody is deleted without
/// having been told why. Likely-illegal content goes to the evidence store
/// first, as with any account deletion.
pub const BAN_DELETION_DAYS: i32 = 183;
/// The reminder email goes out this many days before the deletion.
pub const BAN_DELETION_NOTICE_DAYS: i32 = 14;

/// Bans whose account is still there, not lifted, delivered, and without a
/// pending objection. `$1` = days since the ban.
const DUE_BANS: &str = r#"
    SELECT d.id, u.id, u.email, d.created_at, d.deletion_notified_at
    FROM users u JOIN moderation_decisions d ON d.id = u.suspension_decision_id
    WHERE u.suspended_until = 'infinity' AND d.restriction = 'banned'
      AND d.lifted_at IS NULL AND d.delivered_at IS NOT NULL
      AND d.objection_status IS DISTINCT FROM 'pending'
      AND d.account_deleted_at IS NULL
      AND d.created_at <= NOW() - make_interval(days => $1)
"#;

type DueBan = (Uuid, Uuid, String, DateTime<Utc>, Option<DateTime<Utc>>);

/// Sends due reminders and deletes due accounts. Each step claims its
/// decision row first (UPDATE ... WHERE ... IS NULL), so replicas running
/// the sweep at once don't send twice or delete twice.
pub(crate) async fn sweep_bans(state: &AppState) {
    let reminders = sqlx::query_as::<_, DueBan>(DUE_BANS)
        .bind(BAN_DELETION_DAYS - BAN_DELETION_NOTICE_DAYS)
        .fetch_all(&state.db)
        .await;
    match reminders {
        Ok(rows) => {
            for (decision_id, _, email, banned_at, notified) in rows {
                if notified.is_some() {
                    continue;
                }
                let claimed = sqlx::query(
                    "UPDATE moderation_decisions SET deletion_notified_at = NOW() WHERE id = $1 AND deletion_notified_at IS NULL",
                )
                .bind(decision_id)
                .execute(&state.db)
                .await
                .map(|r| r.rows_affected() > 0);
                if !matches!(claimed, Ok(true)) {
                    continue;
                }
                // The date shown is the planned one, at least 14 days out;
                // deletion also waits 14 days after the reminder itself.
                let planned = banned_at + chrono::Duration::days(BAN_DELETION_DAYS as i64);
                let earliest = Utc::now() + chrono::Duration::days(BAN_DELETION_NOTICE_DAYS as i64);
                let on = planned.max(earliest).format("%d.%m.%Y").to_string();
                if let Err(e) = state.email.send_ban_deletion_notice(&email, decision_id, &on).await {
                    tracing::error!("Ban deletion reminder for decision {} failed: {}", decision_id, e.0);
                }
            }
        }
        Err(e) => tracing::error!("Ban sweep: listing reminders failed: {}", e),
    }

    let due = sqlx::query_as::<_, DueBan>(DUE_BANS)
        .bind(BAN_DELETION_DAYS)
        .fetch_all(&state.db)
        .await;
    let due = match due {
        Ok(rows) => rows,
        Err(e) => {
            tracing::error!("Ban sweep: listing due deletions failed: {}", e);
            return;
        }
    };
    let reminder_cutoff = Utc::now() - chrono::Duration::days(BAN_DELETION_NOTICE_DAYS as i64);
    for (decision_id, user_id, _, _, notified) in due {
        // Only once the reminder is at least two weeks old.
        if notified.is_none_or(|n| n > reminder_cutoff) {
            continue;
        }
        let claimed = sqlx::query(
            "UPDATE moderation_decisions SET account_deleted_at = NOW() WHERE id = $1 AND account_deleted_at IS NULL",
        )
        .bind(decision_id)
        .execute(&state.db)
        .await
        .map(|r| r.rows_affected() > 0);
        if !matches!(claimed, Ok(true)) {
            continue;
        }
        match crate::handlers::users::delete_user(state, user_id, None).await {
            Ok(()) => tracing::info!("Deleted permanently suspended account {} (decision {})", user_id, decision_id),
            Err(e) => {
                tracing::error!("Deleting permanently suspended account {} failed: {}", user_id, e.message);
                let _ = sqlx::query("UPDATE moderation_decisions SET account_deleted_at = NULL WHERE id = $1")
                    .bind(decision_id)
                    .execute(&state.db)
                    .await;
            }
        }
    }
}

/// Requests a suspended account may still make: reading, deleting its own
/// things (content, follows, blocks, the account itself), objecting,
/// reporting (DSA Art. 16 notices are open to everyone), security and
/// feedback. Everything else that writes is refused, including endpoints
/// added later, unless listed here.
fn allowed_while_suspended(method: &Method, path: &str) -> bool {
    if matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS | Method::DELETE) {
        return true;
    }
    if path.starts_with("/admin/") {
        return true;
    }
    matches!(
        (method.as_str(), path),
        ("POST", "/moderation/decisions/{decision_id}/objection")
            | ("POST", "/reports")
            | ("POST", "/feedback")
            | ("POST", "/events")
            | ("POST", "/notifications/stream-ticket")
            | ("PATCH", "/notifications/read")
            | ("PATCH", "/chats/{conversation_id}/read")
            | ("PATCH", "/users/me/password")
            | ("PATCH", "/users/me/keep-account")
            | ("POST", "/users/{username}/block")
            | ("POST", "/users/me/follow-requests/{requester_username}/reject")
    )
}

/// Middleware: refuses writes from suspended accounts (see
/// allowed_while_suspended). One indexed lookup per write request.
pub async fn enforce_suspension(
    State(state): State<AppState>,
    auth: OptionalAuthUser,
    matched: Option<MatchedPath>,
    req: Request,
    next: Next,
) -> Response {
    let path = matched.as_ref().map(|m| m.as_str()).unwrap_or_else(|| req.uri().path());
    if let Some(user_id) = auth.user_id {
        if !allowed_while_suspended(req.method(), path) {
            let mut conn = match state.db.acquire().await {
                Ok(conn) => conn,
                Err(e) => return AppError::internal(format!("Database error: {}", e)).into_response(),
            };
            match suspension(&mut conn, user_id).await {
                Ok(None) => {}
                Ok(Some(s)) => return suspended_error(&s).into_response(),
                Err(e) => return e.into_response(),
            }
        }
    }
    next.run(req).await
}

fn suspended_error(s: &Suspension) -> AppError {
    match s.until {
        Some(until) if !s.permanent => AppError::forbidden(format!(
            "Your account is suspended until {}. You can still read, object to the decision, export your data or delete your account.",
            until.format("%Y-%m-%d %H:%M UTC")
        )),
        _ => AppError::forbidden(
            "Your account is permanently suspended. You can still read, object to the decision, export your data or delete your account.",
        ),
    }
}

/// Runs once an hour: deletes expired strikes (with their snapshots) and
/// snapshot view logs older than a year, and handles due bans
/// (sweep_bans). Safe on every replica.
pub fn spawn_sweeper(state: AppState) {
    tokio::spawn(async move {
        let db = state.db.clone();
        loop {
            sweep_bans(&state).await;
            match sqlx::query("DELETE FROM account_strikes WHERE expires_at <= NOW()").execute(&db).await {
                Ok(r) if r.rows_affected() > 0 => tracing::info!("Deleted {} expired strike(s)", r.rows_affected()),
                Ok(_) => {}
                Err(e) => tracing::error!("Strike sweep failed: {}", e),
            }
            if let Err(e) = sqlx::query("DELETE FROM strike_views WHERE viewed_at < NOW() - INTERVAL '1 year'")
                .execute(&db)
                .await
            {
                tracing::error!("Strike view log sweep failed: {}", e);
            }
            // Account reviews (handlers/account_review.rs) are kept a year.
            if let Err(e) = sqlx::query("DELETE FROM account_reviews WHERE opened_at < NOW() - INTERVAL '1 year'")
                .execute(&db)
                .await
            {
                tracing::error!("Account review sweep failed: {}", e);
            }
            tokio::time::sleep(std::time::Duration::from_secs(60 * 60)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_few_minor_insults_stay_below_a_warning() {
        let minor = Severity::Minor.points();
        let score = 2 * minor + with_repeat_factor(minor);
        assert!(score < 25);
        assert_eq!(Measure::for_score(score), None);
    }

    #[test]
    fn severe_reaches_the_maximum_and_never_expires() {
        assert_eq!(Measure::for_score(Severity::Severe.points()), Some(Measure::Ban));
        assert_eq!(Severity::Severe.lifetime_days(), None);
        assert_eq!(default_violation("csam").unwrap().severity, Severity::Severe);
    }

    #[test]
    fn catalog_covers_every_report_reason_with_unique_ids() {
        for reason in crate::handlers::reports::VALID_REASONS {
            assert!(default_violation(reason).is_some(), "no violation type for {reason}");
        }
        let mut ids: Vec<_> = VIOLATIONS.iter().map(|v| v.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), VIOLATIONS.len());
        assert!(violation("none").is_none(), "'none' means no strike");
        assert_eq!(violation("bot_account").unwrap().severity, Severity::Severe);
        let glorifying = violation("extremism_glorifying").unwrap().severity.points();
        assert_eq!(suggest(glorifying, true, false), Some(Measure::Warning), "hard, but not an instant ban");
        assert_eq!(suggest((2 * glorifying).min(MAX_SCORE), true, true), Some(Measure::Ban));
        let harassment = violation("sexual_harassment").unwrap().severity.points();
        assert_eq!(suggest(harassment, true, false), Some(Measure::Warning));
        assert_eq!(suggest((2 * harassment).min(MAX_SCORE), true, true), Some(Measure::Ban));
        for id in ["bot_account", "terror_propaganda", "terror_threat", "extremism_promotion"] {
            assert_eq!(suggest(violation(id).unwrap().severity.points(), true, false), Some(Measure::Ban), "{id}");
        }
    }

    #[test]
    fn departing_from_the_report_needs_a_justification() {
        assert_eq!(classify("harassment", None, None).unwrap().id(), "insult");
        assert_eq!(classify("harassment", Some("threat_stalking"), None).unwrap().id(), "threat_stalking");
        assert!(classify("terrorism", Some("insult"), None).is_err());
        assert_eq!(classify("terrorism", Some("insult"), Some("Exaggerated report")).unwrap().id(), "insult");
        assert!(classify("spam", Some("none"), Some("  ")).is_err());
        assert!(classify("spam", Some("none"), Some("Compromised account")).unwrap().violation.is_none());
        assert!(classify("spam", Some("nope"), Some("x")).is_err());
    }

    #[test]
    fn repeat_factor_rounds_up_and_caps() {
        assert_eq!(with_repeat_factor(5), 8);
        assert_eq!(with_repeat_factor(20), 30);
        assert_eq!(with_repeat_factor(100), 100);
    }

    #[test]
    fn suspension_needs_a_prior_warning() {
        assert_eq!(suggest(55, true, false), Some(Measure::Warning));
        assert_eq!(suggest(55, true, true), Some(Measure::Suspend7));
        assert_eq!(suggest(80, true, true), Some(Measure::Suspend30));
        assert_eq!(suggest(100, true, false), Some(Measure::Ban));
        assert_eq!(suggest(55, false, true), None, "nothing new since the last measure");
        assert_eq!(suggest(10, true, false), None);
    }

    #[test]
    fn measures_parse() {
        for m in ["warning", "suspend_7d", "suspend_30d", "ban"] {
            assert_eq!(Measure::parse(m).unwrap().as_str(), m);
        }
    }

    #[test]
    fn suspended_accounts_can_still_leave_and_object() {
        assert!(allowed_while_suspended(&Method::DELETE, "/users/me"));
        assert!(allowed_while_suspended(&Method::GET, "/users/me/export"));
        assert!(allowed_while_suspended(&Method::POST, "/moderation/decisions/{decision_id}/objection"));
        assert!(!allowed_while_suspended(&Method::POST, "/posts/upload"));
        assert!(!allowed_while_suspended(&Method::POST, "/chats/send"));
        assert!(!allowed_while_suspended(&Method::PATCH, "/users/me"));
    }
}
