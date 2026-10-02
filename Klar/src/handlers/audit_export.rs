//! Audit export (admin only): the moderation records of a period as a ZIP
//! of CSV files with a summary and a README, for an authority's request
//! (the Bundesnetzagentur as Digital Services Coordinator, Art. 51 DSA; the
//! data protection authority, Art. 58 GDPR) or a voluntary transparency
//! report. Nobody outside the team gets access to the database itself.
//!
//! What goes in is chosen for that purpose (Art. 5(1)(c) GDPR):
//! - Metadata, the statements of reasons and the team's own notes; no
//!   content (no captions, comments, messages, report texts, objection
//!   texts, removed profile fields) and no email addresses. Preserved
//!   content stays in the evidence records, opened one at a time.
//! - Accounts and items as pseudonyms, stable within one export and
//!   different in the next (a fresh key each time), so two exports can't be
//!   linked. With identities, usernames and item IDs instead; that needs the
//!   same reason as every export and is logged as such.
//! - Team members by username: the export is about the team's work.
//!
//! Every export is logged in audit_exports (who, which period, whether with
//! identities, why) before anything is handed out, and the log is part of
//! the next export.

use std::collections::{HashMap, HashSet};
use std::io::{Seek, Write};

use axum::{extract::State, http::StatusCode, response::Response, Json};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use ring::hmac;
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::errors::AppError;
use crate::handlers::auth::AppState;
use crate::handlers::reports::require_admin;
use crate::utils::DbResultExt;

pub const REASON_MIN: usize = 5;
pub const REASON_MAX: usize = 1000;

/// How a column's value is written.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Plain,
    /// A user account: a pseudonym, or with identities its username.
    Account,
    /// A team member: always the username.
    Staff,
    /// What a report or decision is about: an account when the row's
    /// target_type is "user", otherwise an item (post, comment, message).
    Target,
    /// Names of people outside the team who aren't (or needn't be) Klar
    /// users, e.g. a notifier: only with identities, empty otherwise.
    IdentityOnly,
}

use Kind::*;

struct Sheet {
    file: &'static str,
    /// $1 = start of the period, $2 = end (exclusive); $3 = this export's
    /// id, only where `binds_export` is set.
    sql: &'static str,
    order_by: &'static str,
    binds_export: bool,
    columns: &'static [(&'static str, Kind)],
}

const SHEETS: &[Sheet] = &[
    Sheet {
        file: "entscheidungen.csv",
        sql: r#"
            SELECT d.id AS decision_id, d.created_at, d.target_type::text AS target_type, d.target_id AS target,
                   d.affected_user_id AS affected_account, d.restriction, d.automated, d.reason::text AS reason,
                   d.violation_type, d.source, d.ground_type, d.ground, d.explanation AS statement,
                   cardinality(d.report_ids) AS report_count, d.report_ids, d.rights_claim_id, d.decided_by,
                   d.delivered_at, d.suspension_days, d.standing_score, d.lifted_at, d.lifted_by, d.superseded_by,
                   d.objected_at, d.objection_status, d.objection_response, d.objection_resolved_at,
                   d.objection_resolved_by, d.content_purged_at, d.account_deleted_at
            FROM moderation_decisions d WHERE d.created_at >= $1 AND d.created_at < $2
        "#,
        order_by: "created_at, decision_id",
        binds_export: false,
        columns: &[
            ("decision_id", Plain), ("created_at", Plain), ("target_type", Plain), ("target", Target),
            ("affected_account", Account), ("restriction", Plain), ("automated", Plain), ("reason", Plain),
            ("violation_type", Plain), ("source", Plain), ("ground_type", Plain), ("ground", Plain),
            ("statement", Plain), ("report_count", Plain), ("report_ids", Plain), ("rights_claim_id", Plain),
            ("decided_by", Staff), ("delivered_at", Plain), ("suspension_days", Plain), ("standing_score", Plain),
            ("lifted_at", Plain), ("lifted_by", Staff), ("superseded_by", Plain), ("objected_at", Plain),
            ("objection_status", Plain), ("objection_response", Plain), ("objection_resolved_at", Plain),
            ("objection_resolved_by", Staff), ("content_purged_at", Plain), ("account_deleted_at", Plain),
        ],
    },
    Sheet {
        file: "meldungen.csv",
        sql: r#"
            SELECT r.id AS report_id, r.created_at, r.source, r.target_type::text AS target_type, r.target_id AS target,
                   r.reason::text AS reason, r.reporter_id AS reporter, r.notice_id, r.authority, r.order_reference,
                   r.status::text AS status, r.outcome, r.reviewed_at, r.reviewed_by, r.review_note,
                   round((EXTRACT(EPOCH FROM r.reviewed_at - r.created_at) / 3600)::numeric, 1) AS hours_to_decision,
                   r.recheck_requested_at
            FROM reports r WHERE r.created_at >= $1 AND r.created_at < $2
        "#,
        order_by: "created_at, report_id",
        binds_export: false,
        columns: &[
            ("report_id", Plain), ("created_at", Plain), ("source", Plain), ("target_type", Plain), ("target", Target),
            ("reason", Plain), ("reporter", Account), ("notice_id", Plain), ("authority", Plain),
            ("order_reference", Plain), ("status", Plain), ("outcome", Plain), ("reviewed_at", Plain),
            ("reviewed_by", Staff), ("review_note", Plain), ("hours_to_decision", Plain),
            ("recheck_requested_at", Plain),
        ],
    },
    Sheet {
        file: "hinweise.csv",
        sql: r#"
            SELECT n.id AS notice_id, n.created_at, n.reason::text AS reason, n.target_type::text AS target_type,
                   n.target_id AS target, n.notifier_user_id AS notifier_account,
                   CASE WHEN n.notifier_user_id IS NULL THEN n.notifier_name END AS notifier_name,
                   n.good_faith, n.decided_at, n.outcome
            FROM content_notices n WHERE n.created_at >= $1 AND n.created_at < $2
        "#,
        order_by: "created_at, notice_id",
        binds_export: false,
        columns: &[
            ("notice_id", Plain), ("created_at", Plain), ("reason", Plain), ("target_type", Plain), ("target", Target),
            ("notifier_account", Account), ("notifier_name", IdentityOnly), ("good_faith", Plain),
            ("decided_at", Plain), ("outcome", Plain),
        ],
    },
    Sheet {
        file: "rechteverletzungen.csv",
        sql: r#"
            SELECT c.id AS claim_id, c.created_at, c.claim_type, c.target_type::text AS target_type,
                   c.target_id AS target, c.claimant_user_id AS claimant_account, c.claimant_name,
                   c.claimant_organization, c.represented_party, c.good_faith, c.status, c.decided_at, c.decided_by,
                   c.decision_reason, c.decision_id
            FROM rights_claims c WHERE c.created_at >= $1 AND c.created_at < $2
        "#,
        order_by: "created_at, claim_id",
        binds_export: false,
        columns: &[
            ("claim_id", Plain), ("created_at", Plain), ("claim_type", Plain), ("target_type", Plain),
            ("target", Target), ("claimant_account", Account), ("claimant_name", IdentityOnly),
            ("claimant_organization", IdentityOnly), ("represented_party", IdentityOnly), ("good_faith", Plain),
            ("status", Plain), ("decided_at", Plain), ("decided_by", Staff), ("decision_reason", Plain),
            ("decision_id", Plain),
        ],
    },
    // A note without an actor came from the claimant: their words stay out.
    Sheet {
        file: "rechteverletzungen_verlauf.csv",
        sql: r#"
            SELECT e.id AS event_id, e.claim_id, e.created_at, e.action, e.actor_id AS actor,
                   CASE WHEN e.actor_id IS NOT NULL THEN e.note END AS note
            FROM rights_claim_events e WHERE e.created_at >= $1 AND e.created_at < $2
        "#,
        order_by: "created_at, event_id",
        binds_export: false,
        columns: &[
            ("claim_id", Plain), ("created_at", Plain), ("action", Plain), ("actor", Staff), ("note", Plain),
        ],
    },
    Sheet {
        file: "beweissicherung.csv",
        sql: r#"
            SELECT e.id AS evidence_id, e.created_at, e.target_type::text AS target_type, e.target_id AS target,
                   e.reasons, e.content_deleted_at, e.deletion_trigger, e.decision, e.decided_at, e.decided_by,
                   e.decision_note, e.retain_until, e.legal_hold, e.purged_at, e.authority_report
            FROM evidence_records e WHERE e.created_at >= $1 AND e.created_at < $2
        "#,
        order_by: "created_at, evidence_id",
        binds_export: false,
        columns: &[
            ("evidence_id", Plain), ("created_at", Plain), ("target_type", Plain), ("target", Target),
            ("reasons", Plain), ("content_deleted_at", Plain), ("deletion_trigger", Plain), ("decision", Plain),
            ("decided_at", Plain), ("decided_by", Staff), ("decision_note", Plain), ("retain_until", Plain),
            ("legal_hold", Plain), ("purged_at", Plain), ("authority_report", Plain),
        ],
    },
    Sheet {
        file: "beweissicherung_protokoll.csv",
        sql: r#"
            SELECT ev.id AS event_id, ev.evidence_id, ev.created_at, ev.action, ev.actor_id AS actor, ev.reason
            FROM evidence_events ev WHERE ev.created_at >= $1 AND ev.created_at < $2
        "#,
        order_by: "created_at, event_id",
        binds_export: false,
        columns: &[
            ("evidence_id", Plain), ("created_at", Plain), ("action", Plain), ("actor", Staff), ("reason", Plain),
        ],
    },
    Sheet {
        file: "kontosperren.csv",
        sql: r#"
            SELECT l.id AS lock_id, l.user_id AS account, l.locked_at, l.locked_by, l.note, l.assessment,
                   l.assessed_by, l.assessed_at, l.links_sent, l.unlocked_at, l.unlocked_via, l.unlocked_by
            FROM account_locks l WHERE l.locked_at >= $1 AND l.locked_at < $2
        "#,
        order_by: "locked_at, lock_id",
        binds_export: false,
        columns: &[
            ("lock_id", Plain), ("account", Account), ("locked_at", Plain), ("locked_by", Staff), ("note", Plain),
            ("assessment", Plain), ("assessed_by", Staff), ("assessed_at", Plain), ("links_sent", Plain),
            ("unlocked_at", Plain), ("unlocked_via", Plain), ("unlocked_by", Staff),
        ],
    },
    Sheet {
        file: "kontopruefungen.csv",
        sql: r#"
            SELECT v.id AS review_id, v.user_id AS account, v.reviewer_id AS reviewer, v.reason, v.report_id,
                   v.opened_at, v.outcome, v.outcome_note, v.decided_at
            FROM account_reviews v WHERE v.opened_at >= $1 AND v.opened_at < $2
        "#,
        order_by: "opened_at, review_id",
        binds_export: false,
        columns: &[
            ("review_id", Plain), ("account", Account), ("reviewer", Staff), ("reason", Plain), ("report_id", Plain),
            ("opened_at", Plain), ("outcome", Plain), ("outcome_note", Plain), ("decided_at", Plain),
        ],
    },
    Sheet {
        file: "verstoesse_einsicht.csv",
        sql: r#"
            SELECT s.id AS view_id, s.strike_id, s.user_id AS account, s.admin_id AS viewed_by, s.viewed_at
            FROM strike_views s WHERE s.viewed_at >= $1 AND s.viewed_at < $2
        "#,
        order_by: "viewed_at, view_id",
        binds_export: false,
        columns: &[("strike_id", Plain), ("account", Account), ("viewed_by", Staff), ("viewed_at", Plain)],
    },
    // This export is always listed, also when it lies outside its period.
    Sheet {
        file: "exporte.csv",
        sql: r#"
            SELECT x.id AS export_id, x.created_at, x.exported_by, x.period_from, x.period_to, x.with_identities,
                   x.reason
            FROM audit_exports x WHERE (x.created_at >= $1 AND x.created_at < $2) OR x.id = $3
        "#,
        order_by: "created_at, export_id",
        binds_export: true,
        columns: &[
            ("export_id", Plain), ("created_at", Plain), ("exported_by", Staff), ("period_from", Plain),
            ("period_to", Plain), ("with_identities", Plain), ("reason", Plain),
        ],
    },
];

/// The figures for the period: what came in, what was decided, how fast.
/// Each counts what happened in the period (a report by when it came in or
/// was decided, an objection by when it was raised).
const SUMMARY_SQL: &str = r#"
    WITH r AS (SELECT * FROM reports WHERE created_at >= $1 AND created_at < $2),
         rd AS (SELECT * FROM reports WHERE reviewed_at >= $1 AND reviewed_at < $2),
         d AS (SELECT * FROM moderation_decisions WHERE created_at >= $1 AND created_at < $2),
         o AS (SELECT * FROM moderation_decisions WHERE objected_at >= $1 AND objected_at < $2),
         c AS (SELECT * FROM rights_claims WHERE created_at >= $1 AND created_at < $2)
    SELECT section, metric, value FROM (
        SELECT 1 AS ord, 'Meldungen' AS section, 'eingegangen' AS metric, COUNT(*)::text AS value FROM r
        UNION ALL SELECT 2, 'Meldungen', 'eingegangen, Quelle: ' || source, COUNT(*)::text FROM r GROUP BY source
        UNION ALL SELECT 3, 'Meldungen', 'eingegangen, Grund: ' || reason::text, COUNT(*)::text FROM r GROUP BY reason
        UNION ALL SELECT 4, 'Meldungen', 'entschieden', COUNT(*)::text FROM rd
        UNION ALL SELECT 5, 'Meldungen', 'entschieden, Ergebnis: ' || COALESCE(outcome, status::text), COUNT(*)::text
            FROM rd GROUP BY COALESCE(outcome, status::text)
        UNION ALL SELECT 6, 'Meldungen', 'Stunden bis zur Entscheidung, Median',
            COALESCE(round((percentile_cont(0.5) WITHIN GROUP (ORDER BY EXTRACT(EPOCH FROM reviewed_at - created_at)) / 3600)::numeric, 1)::text, '')
            FROM rd
        UNION ALL SELECT 7, 'Meldungen', 'Stunden bis zur Entscheidung, 90. Perzentil',
            COALESCE(round((percentile_cont(0.9) WITHIN GROUP (ORDER BY EXTRACT(EPOCH FROM reviewed_at - created_at)) / 3600)::numeric, 1)::text, '')
            FROM rd
        UNION ALL SELECT 8, 'Hinweise über das öffentliche Formular', 'eingegangen', COUNT(*)::text
            FROM content_notices WHERE created_at >= $1 AND created_at < $2
        UNION ALL SELECT 10, 'Entscheidungen', 'getroffen', COUNT(*)::text FROM d
        UNION ALL SELECT 11, 'Entscheidungen', 'automatisch', (COUNT(*) FILTER (WHERE automated))::text FROM d
        UNION ALL SELECT 12, 'Entscheidungen', 'durch das Team', (COUNT(*) FILTER (WHERE NOT automated))::text FROM d
        UNION ALL SELECT 13, 'Entscheidungen', 'Maßnahme: ' || restriction, COUNT(*)::text FROM d GROUP BY restriction
        UNION ALL SELECT 14, 'Entscheidungen', 'Anlass: ' || COALESCE(source, 'unbekannt'), COUNT(*)::text
            FROM d GROUP BY source
        UNION ALL SELECT 15, 'Entscheidungen', 'Grund: ' || reason::text, COUNT(*)::text FROM d GROUP BY reason
        UNION ALL SELECT 16, 'Entscheidungen', 'Begründung noch nicht zugestellt',
            (COUNT(*) FILTER (WHERE delivered_at IS NULL))::text FROM d
        UNION ALL SELECT 20, 'Widersprüche', 'eingegangen', COUNT(*)::text FROM o
        UNION ALL SELECT 21, 'Widersprüche', 'Stand: ' || objection_status, COUNT(*)::text FROM o GROUP BY objection_status
        UNION ALL SELECT 22, 'Widersprüche', 'Stunden bis zur Antwort, Median',
            COALESCE(round((percentile_cont(0.5) WITHIN GROUP (ORDER BY EXTRACT(EPOCH FROM objection_resolved_at - objected_at)) / 3600)::numeric, 1)::text, '')
            FROM o WHERE objection_resolved_at IS NOT NULL
        UNION ALL SELECT 30, 'Rechteverletzungen', 'eingegangen', COUNT(*)::text FROM c
        UNION ALL SELECT 31, 'Rechteverletzungen', 'Stand: ' || status, COUNT(*)::text FROM c GROUP BY status
        UNION ALL SELECT 40, 'Beweissicherung', 'angelegt', COUNT(*)::text
            FROM evidence_records WHERE created_at >= $1 AND created_at < $2
        UNION ALL SELECT 41, 'Beweissicherung', 'Protokoll: ' || action, COUNT(*)::text
            FROM evidence_events WHERE created_at >= $1 AND created_at < $2 GROUP BY action
        UNION ALL SELECT 50, 'Konten', 'zum Schutz gesperrt', COUNT(*)::text
            FROM account_locks WHERE locked_at >= $1 AND locked_at < $2
        UNION ALL SELECT 51, 'Konten', 'geprüft', COUNT(*)::text
            FROM account_reviews WHERE opened_at >= $1 AND opened_at < $2
    ) s ORDER BY ord, metric
"#;

/// Turns accounts and items into what the export shows.
struct Names {
    key: hmac::Key,
    with_identities: bool,
    usernames: HashMap<Uuid, String>,
}

impl Names {
    /// A short code from a keyed hash: the same ID gets the same code
    /// within this export; without the key (discarded afterwards) it can't
    /// be traced back or matched with another export.
    fn code(&self, prefix: &str, id: Uuid) -> String {
        let tag = hmac::sign(&self.key, id.as_bytes());
        format!("{}-{}", prefix, hex::encode_upper(&tag.as_ref()[..5]))
    }

    fn account(&self, id: Uuid) -> String {
        if !self.with_identities {
            return self.code("K", id);
        }
        match self.usernames.get(&id) {
            Some(name) => format!("@{}", name),
            None => format!("gelöschtes Konto {}", self.code("K", id)),
        }
    }

    fn staff(&self, id: Uuid) -> String {
        match self.usernames.get(&id) {
            Some(name) => format!("@{}", name),
            None => "ehemaliges Teammitglied".to_string(),
        }
    }

    fn item(&self, id: Uuid) -> String {
        if self.with_identities { id.to_string() } else { self.code("I", id) }
    }
}

/// A JSON value as cell text. Text that Excel would read as a formula
/// (=, +, -, @, tab, CR at the start) gets a leading apostrophe, so a note
/// can't run as one on the auditor's machine.
fn cell_text(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::Bool(true) => "ja".to_string(),
        Value::Bool(false) => "nein".to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => {
            if s.starts_with(['=', '+', '-', '@', '\t', '\r']) { format!("'{}", s) } else { s.clone() }
        }
        Value::Array(items) => items.iter().map(cell_text).collect::<Vec<_>>().join(", "),
        Value::Object(_) => value.to_string(),
    }
}

fn uuid_of(value: &Value) -> Option<Uuid> {
    value.as_str().and_then(|s| s.parse().ok())
}

/// One CSV line (RFC 4180, with semicolons, which German Excel expects).
fn csv_line(cells: impl IntoIterator<Item = String>) -> String {
    let quoted: Vec<String> = cells
        .into_iter()
        .map(|c| if c.contains([';', '"', '\n', '\r']) { format!("\"{}\"", c.replace('"', "\"\"")) } else { c })
        .collect();
    format!("{}\r\n", quoted.join(";"))
}

/// UTF-8 with a byte order mark, so Excel shows umlauts right.
const BOM: &str = "\u{feff}";

fn render(sheet: &Sheet, rows: &[Value], names: &Names) -> String {
    let mut out = String::from(BOM);
    out.push_str(&csv_line(sheet.columns.iter().map(|(name, _)| name.to_string())));
    for row in rows {
        out.push_str(&csv_line(sheet.columns.iter().map(|&(name, kind)| {
            let value = &row[name];
            match kind {
                Plain => cell_text(value),
                IdentityOnly if names.with_identities => cell_text(value),
                IdentityOnly => String::new(),
                Account => uuid_of(value).map(|id| names.account(id)).unwrap_or_default(),
                Staff => uuid_of(value).map(|id| names.staff(id)).unwrap_or_default(),
                Target => match (uuid_of(value), row["target_type"].as_str()) {
                    (Some(id), Some("user")) => names.account(id),
                    (Some(id), _) => names.item(id),
                    (None, _) => String::new(),
                },
            }
        })));
    }
    out
}

/// Every account or team member the rows mention, for looking up usernames.
fn people(sheet: &Sheet, rows: &[Value], into: &mut HashSet<Uuid>) {
    for row in rows {
        for &(name, kind) in sheet.columns {
            let person = match kind {
                Account | Staff => true,
                Target => row["target_type"].as_str() == Some("user"),
                Plain | IdentityOnly => false,
            };
            if let Some(id) = uuid_of(&row[name]).filter(|_| person) {
                into.insert(id);
            }
        }
    }
}

async fn rows(conn: &mut PgConnection, sheet: &Sheet, from: DateTime<Utc>, until: DateTime<Utc>, export_id: Uuid) -> Result<Vec<Value>, AppError> {
    let sql = format!("SELECT to_jsonb(q) FROM ({}) q ORDER BY {}", sheet.sql, sheet.order_by);
    let mut query = sqlx::query_scalar::<_, Value>(&sql).bind(from).bind(until);
    if sheet.binds_export {
        query = query.bind(export_id);
    }
    query.fetch_all(&mut *conn).await.db_err_ctx(&format!("Audit export: {} failed", sheet.file), "Database error")
}

fn readme(input: &AuditExportRequest, admin: &str, export_id: Uuid, created_at: DateTime<Utc>) -> String {
    format!(
        r#"Klar — Export für Prüfungen und Auskunftsersuchen
=================================================

Zeitraum:        {from} bis {to} (jeweils einschließlich, UTC)
Erstellt:        {created} (UTC) von @{admin}
Export-ID:       {export_id}
Personen:        {identities}
Anlass:          {reason}

Was enthalten ist
-----------------
Die Moderationsvorgänge, die im Zeitraum angelegt wurden: Meldungen,
Hinweise über das öffentliche Formular, Entscheidungen mit ihren
Begründungen (Art. 17 DSA), Widersprüche, Rechteverletzungen,
Beweissicherungen mit ihrem Zugriffsprotokoll, Kontosperren und
-prüfungen, Einsichtnahmen in Verstöße und frühere Exporte. Spätere
Änderungen (z. B. eine Aufhebung) stehen als Zeitstempel in derselben
Zeile. summary.csv enthält die Kennzahlen des Zeitraums.

Was nicht enthalten ist
-----------------------
Keine Inhalte: keine Beiträge, Kommentare, Nachrichten, Meldungs- oder
Widerspruchstexte und keine E-Mail-Adressen. Gesicherte Inhalte liegen in
den Beweissicherungen und werden einzeln und protokolliert eingesehen.

Personen
--------
Ohne Identitäten erscheinen Konten als K-…, Inhalte als I-…. Die Codes
sind innerhalb dieses Exports stabil (dasselbe Konto hat überall denselben
Code), in jedem Export aber anders; zurückverfolgen lassen sie sich nicht.
Mit Identitäten stehen dort Benutzernamen (@…) und die IDs der Inhalte,
bei Hinweisen und Rechteverletzungen auch die angegebenen Namen.
Teammitglieder erscheinen immer mit Benutzernamen.

Aufbewahrung
------------
Der Export kann nur zeigen, was noch gespeichert ist: Meldungen löschen
wir sechs Monate nach der Entscheidung, Entscheidungen nach drei Jahren
(solange nichts mehr darauf beruht).

Format
------
CSV, UTF-8, Trennzeichen Semikolon, Zeitangaben ISO 8601 in UTC,
ja/nein für Wahrheitswerte. Ein vorangestelltes ' verhindert, dass ein
Text als Formel ausgeführt wird. Die Spaltennamen entsprechen den Feldern
in Klar; Aufzählungen (z. B. restriction, source, outcome) nutzen dieselben
Werte wie die Anwendung.

Dateien
-------
summary.csv                     Kennzahlen: Bereich, Kennzahl, Wert
entscheidungen.csv              Moderationsentscheidungen mit Begründung (statement),
                                Rechtsgrundlage (ground), Anlass (source), Widerspruch
meldungen.csv                   Meldungen aus der App, behördliche Anordnungen, eigene Fälle
hinweise.csv                    Hinweise über das öffentliche Formular
rechteverletzungen.csv          Meldungen von Urheber- und anderen Rechteverletzungen
rechteverletzungen_verlauf.csv  Ihre Bearbeitungsschritte
beweissicherung.csv             Gesicherte Fassungen gemeldeter, möglicherweise
                                rechtswidriger Inhalte (nur Metadaten)
beweissicherung_protokoll.csv   Jeder Zugriff und Schritt, mit Grund
kontosperren.csv                Sperren übernommen wirkender Konten
kontopruefungen.csv             Prüfungen von Konten (z. B. Bots)
verstoesse_einsicht.csv         Wer wann einen Verstoß im Detail angesehen hat
exporte.csv                     Exporte wie dieser, mit Anlass
"#,
        from = input.from,
        to = input.to,
        created = created_at.format("%Y-%m-%d %H:%M:%S"),
        admin = admin,
        export_id = export_id,
        identities = if input.with_identities { "mit Identitäten (Benutzernamen)" } else { "pseudonymisiert" },
        reason = input.reason.trim(),
    )
}

#[derive(Debug, Deserialize)]
pub struct AuditExportRequest {
    pub from: NaiveDate,
    pub to: NaiveDate,
    #[serde(default)]
    pub with_identities: bool,
    pub reason: String,
}

/// POST /admin/audit-exports (admin only) -- logs the export, then returns
/// the ZIP. Everything is read in one REPEATABLE READ transaction, so the
/// files agree with each other and with the log entry they include.
pub async fn create_audit_export(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<AuditExportRequest>,
) -> Result<Response, AppError> {
    require_admin(&state.db, &auth).await?;
    let reason = input.reason.trim();
    if reason.chars().count() < REASON_MIN || reason.chars().count() > REASON_MAX {
        return Err(AppError::bad_request(format!(
            "Give the reason for the export ({} to {} characters): who asked, and what for",
            REASON_MIN, REASON_MAX
        )));
    }
    if input.from > input.to {
        return Err(AppError::bad_request("The period starts after it ends"));
    }
    if input.to > Utc::now().date_naive() {
        return Err(AppError::bad_request("The period can't end in the future"));
    }
    let from = input.from.and_hms_opt(0, 0, 0).unwrap().and_utc();
    let until = (input.to + Duration::days(1)).and_hms_opt(0, 0, 0).unwrap().and_utc();

    let mut key = [0u8; 32];
    SystemRandom::new().fill(&mut key).map_err(|_| AppError::internal("Failed to start the export"))?;

    let mut tx = state.db.begin().await.db_err("Database error")?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ").execute(&mut *tx).await.db_err("Database error")?;
    sqlx::query("SET LOCAL TIME ZONE 'UTC'").execute(&mut *tx).await.db_err("Database error")?;

    let (export_id, created_at) = sqlx::query_as::<_, (Uuid, DateTime<Utc>)>(
        r#"
        INSERT INTO audit_exports (exported_by, period_from, period_to, with_identities, reason)
        VALUES ($1, $2, $3, $4, $5) RETURNING id, created_at
        "#,
    )
    .bind(auth.user_id)
    .bind(input.from)
    .bind(input.to)
    .bind(input.with_identities)
    .bind(reason)
    .fetch_one(&mut *tx)
    .await
    .db_err_ctx("Audit export: logging failed", "Database error")?;

    let mut sheets = Vec::with_capacity(SHEETS.len());
    let mut mentioned = HashSet::from([auth.user_id]);
    for sheet in SHEETS {
        let rows = rows(&mut tx, sheet, from, until, export_id).await?;
        people(sheet, &rows, &mut mentioned);
        sheets.push((sheet, rows));
    }
    let ids: Vec<Uuid> = mentioned.into_iter().collect();
    let usernames: HashMap<Uuid, String> = sqlx::query_as::<_, (Uuid, String)>("SELECT id, username FROM users WHERE id = ANY($1)")
        .bind(&ids)
        .fetch_all(&mut *tx)
        .await
        .db_err("Database error")?
        .into_iter()
        .collect();
    let summary = sqlx::query_as::<_, (String, String, String)>(SUMMARY_SQL)
        .bind(from)
        .bind(until)
        .fetch_all(&mut *tx)
        .await
        .db_err_ctx("Audit export: summary failed", "Database error")?;
    let admin = usernames.get(&auth.user_id).cloned().unwrap_or_default();
    let names = Names { key: hmac::Key::new(hmac::HMAC_SHA256, &key), with_identities: input.with_identities, usernames };

    let tmp_err = |e: std::io::Error| {
        tracing::error!("Audit export temp file error: {}", e);
        AppError::internal("Failed to build the export")
    };
    let zip_err = |e: zip::result::ZipError| {
        tracing::error!("Audit export zip error: {}", e);
        AppError::internal("Failed to build the export")
    };
    let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let mut zip = zip::ZipWriter::new(tempfile::tempfile().map_err(tmp_err)?);
    zip.start_file("README.txt", options).map_err(zip_err)?;
    zip.write_all(readme(&input, &admin, export_id, created_at).as_bytes()).map_err(tmp_err)?;
    let mut summary_csv = String::from(BOM);
    summary_csv.push_str(&csv_line(["section", "metric", "value"].map(String::from)));
    for (section, metric, value) in summary {
        summary_csv.push_str(&csv_line([section, metric, value]));
    }
    zip.start_file("summary.csv", options).map_err(zip_err)?;
    zip.write_all(summary_csv.as_bytes()).map_err(tmp_err)?;
    for (sheet, rows) in &sheets {
        zip.start_file(sheet.file, options).map_err(zip_err)?;
        zip.write_all(render(sheet, rows, &names).as_bytes()).map_err(tmp_err)?;
    }
    let mut file = zip.finish().map_err(zip_err)?;

    // Only now: an export that failed to build isn't logged as handed out.
    tx.commit().await.db_err("Database error")?;
    tracing::info!(
        "Audit export {} by {} for {}..{} (identities: {})",
        export_id, auth.user_id, input.from, input.to, input.with_identities
    );

    let size = file.seek(std::io::SeekFrom::End(0)).map_err(tmp_err)?;
    file.seek(std::io::SeekFrom::Start(0)).map_err(tmp_err)?;
    let body = axum::body::Body::from_stream(tokio_util::io::ReaderStream::new(tokio::fs::File::from_std(file)));
    Response::builder()
        .status(StatusCode::OK)
        .header(axum::http::header::CONTENT_TYPE, "application/zip")
        .header(axum::http::header::CONTENT_LENGTH, size)
        .header(
            axum::http::header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"klar-audit-{}_{}.zip\"", input.from, input.to),
        )
        .body(body)
        .map_err(|_| AppError::internal("Failed to build the export response"))
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct AuditExportEntry {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub exported_by_username: Option<String>,
    pub period_from: NaiveDate,
    pub period_to: NaiveDate,
    pub with_identities: bool,
    pub reason: String,
}

/// GET /admin/audit-exports (admin only) -- the latest exports, newest first.
pub async fn list_audit_exports(State(state): State<AppState>, auth: AuthUser) -> Result<Json<Vec<AuditExportEntry>>, AppError> {
    require_admin(&state.db, &auth).await?;
    let entries = sqlx::query_as::<_, AuditExportEntry>(
        r#"
        SELECT x.id, x.created_at, u.username AS exported_by_username, x.period_from, x.period_to,
               x.with_identities, x.reason
        FROM audit_exports x LEFT JOIN users u ON u.id = x.exported_by
        ORDER BY x.created_at DESC LIMIT 100
        "#,
    )
    .fetch_all(&state.db)
    .await
    .db_err("Database error")?;
    Ok(Json(entries))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cells_are_quoted_and_formulas_defused() {
        assert_eq!(cell_text(&Value::String("=HYPERLINK(\"x\")".into())), "'=HYPERLINK(\"x\")");
        assert_eq!(cell_text(&Value::String("-1".into())), "'-1");
        assert_eq!(cell_text(&serde_json::json!(-1)), "-1", "numbers are no formulas");
        assert_eq!(cell_text(&Value::Bool(true)), "ja");
        assert_eq!(csv_line(["a;b".to_string(), "say \"hi\"".to_string(), "x".to_string()]), "\"a;b\";\"say \"\"hi\"\"\";x\r\n");
    }

    #[test]
    fn pseudonyms_are_stable_within_an_export_and_differ_between_exports() {
        let id = Uuid::new_v4();
        let names = |key: &[u8]| Names { key: hmac::Key::new(hmac::HMAC_SHA256, key), with_identities: false, usernames: HashMap::new() };
        let (a, b) = (names(&[1; 32]), names(&[2; 32]));
        assert_eq!(a.account(id), a.account(id));
        assert!(a.account(id).starts_with("K-"));
        assert_ne!(a.account(id), b.account(id));
        assert!(a.item(id).starts_with("I-"));
    }
}
