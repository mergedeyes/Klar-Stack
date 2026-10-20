//! The language of what the backend says to people: error messages and
//! the account emails (email.rs).
//!
//! Handlers keep writing their messages in English. `negotiate` (a
//! middleware around the whole router) reads the request's Accept-Language
//! -- the frontend sends the language the page is shown in -- and keeps it
//! for the rest of the request; `AppError` and the few JSON messages that
//! reach people pass their text through `tr`, which looks the English text
//! up in the German catalog below. Anything not in it (mostly the admin
//! pages' messages, which are English only) stays English.
//!
//! An entry's English side must match the message in the code exactly; a
//! unit test checks that every entry still occurs in the source, so
//! rewording a message without its entry fails the tests instead of
//! quietly falling back to English.

use std::collections::HashMap;
use std::sync::LazyLock;

use axum::{extract::Request, http::header, middleware::Next, response::Response};

/// The languages Klar is translated into, as validation::LOCALES.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    En,
    De,
}

impl Lang {
    /// An account's stored language ("en" / "de"), if it has one.
    pub fn from_locale(locale: Option<&str>) -> Option<Lang> {
        match locale? {
            "en" => Some(Lang::En),
            "de" => Some(Lang::De),
            _ => None,
        }
    }

    /// The most preferred language Klar has from an Accept-Language header
    /// such as "de-AT,de;q=0.9,en;q=0.8", English when it asks for neither;
    /// the same rule as the frontend's negotiateLocale (i18n/config.ts).
    pub fn negotiate(header: Option<&str>) -> Lang {
        let Some(header) = header else { return Lang::En };
        let mut ranked: Vec<(f32, usize, &str)> = header
            .split(',')
            .enumerate()
            .filter_map(|(index, part)| {
                let mut pieces = part.split(';').map(str::trim);
                let tag = pieces.next()?;
                let weight = pieces
                    .find_map(|p| p.strip_prefix("q="))
                    .map_or(1.0, |q| q.parse().unwrap_or(0.0));
                let lang = tag.split('-').next()?;
                (weight > 0.0).then_some((weight, index, lang))
            })
            .collect();
        ranked.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
        ranked
            .iter()
            .find_map(|(_, _, lang)| Lang::from_locale(Some(lang.to_ascii_lowercase().as_str())))
            .unwrap_or(Lang::En)
    }
}

tokio::task_local! {
    static LANG: Lang;
}

/// The language of an email to an account: the account's own, or
/// `fallback` while it has none (accounts from before languages existed,
/// until the app sets it). Emails the person asked for just now fall back
/// to the page's language (`current()`), the others to German, which all
/// emails were before.
pub fn for_account(locale: Option<&str>, fallback: Lang) -> Lang {
    Lang::from_locale(locale).unwrap_or(fallback)
}

/// The language of the request being handled; English outside one (the
/// background tasks).
pub fn current() -> Lang {
    LANG.try_with(|lang| *lang).unwrap_or(Lang::En)
}

/// Middleware: the request's language, for everything that runs while it
/// is handled.
pub async fn negotiate(req: Request, next: Next) -> Response {
    let lang = Lang::negotiate(req.headers().get(header::ACCEPT_LANGUAGE).and_then(|v| v.to_str().ok()));
    LANG.scope(lang, next.run(req)).await
}

/// `message` in the current request's language.
pub fn tr(message: &str) -> String {
    tr_in(current(), message)
}

/// `message` in `lang`: the catalog's translation, or the message itself.
pub fn tr_in(lang: Lang, message: &str) -> String {
    match lang {
        Lang::En => message.to_string(),
        Lang::De => german(message).unwrap_or_else(|| message.to_string()),
    }
}

fn german(message: &str) -> Option<String> {
    if let Some(de) = EXACT.get(message) {
        return Some((*de).to_string());
    }
    TEMPLATES.iter().find_map(|(en, de)| {
        let args = match_template(en, message)?;
        // A field name in a generic message ("Caption cannot be empty")
        // is translated too; numbers and other values pass through.
        let args: Vec<&str> = args.iter().map(|a| FIELDS.get(a).copied().unwrap_or(a)).collect();
        Some(fill(de, &args))
    })
}

/// The values a message fills into `template`'s `{}` placeholders, or None
/// if it doesn't have the template's shape.
fn match_template<'a>(template: &str, message: &'a str) -> Option<Vec<&'a str>> {
    let parts: Vec<&str> = template.split("{}").collect();
    let mut rest = message.strip_prefix(parts[0])?;
    let mut args = Vec::with_capacity(parts.len() - 1);
    for (i, part) in parts.iter().enumerate().skip(1) {
        let last = i == parts.len() - 1;
        let end = if last {
            rest.strip_suffix(part)?.len()
        } else if part.is_empty() {
            return None; // two placeholders in a row can't be told apart
        } else {
            rest.find(part)?
        };
        if end == 0 {
            return None;
        }
        args.push(&rest[..end]);
        rest = &rest[end + part.len()..];
    }
    Some(args)
}

/// `template` with each `{}` replaced by the next value.
fn fill(template: &str, args: &[&str]) -> String {
    let mut out = String::with_capacity(template.len() + 16);
    let mut args = args.iter();
    let mut pieces = template.split("{}");
    if let Some(first) = pieces.next() {
        out.push_str(first);
    }
    for piece in pieces {
        out.push_str(args.next().copied().unwrap_or(""));
        out.push_str(piece);
    }
    out
}

static EXACT: LazyLock<HashMap<&'static str, &'static str>> =
    LazyLock::new(|| CATALOG.iter().filter(|(en, _)| !en.contains("{}")).copied().collect());

/// The entries with placeholders, most specific first: "Password must be {}
/// characters or less" has to win over "{} must be {} characters or less".
static TEMPLATES: LazyLock<Vec<(&'static str, &'static str)>> = LazyLock::new(|| {
    let mut templates: Vec<_> = CATALOG.iter().filter(|(en, _)| en.contains("{}")).copied().collect();
    templates.sort_by_key(|(en, _)| std::cmp::Reverse(en.replace("{}", "").len()));
    templates
});

static FIELDS: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| FIELD_NAMES.iter().copied().collect());

/// Field names handed to validation::required_text / check_max_len and the
/// like, for the generic messages about them.
const FIELD_NAMES: &[(&str, &str)] = &[
    ("Basis of your rights", "Die Grundlage deiner Rechte"),
    ("Bio", "Die Bio"),
    ("Caption", "Die Bildunterschrift"),
    ("Comment", "Der Kommentar"),
    ("Description of the work", "Die Beschreibung des Werks"),
    ("Display name", "Der Anzeigename"),
    ("Link", "Der Link"),
    ("Link to the post", "Der Link zum Beitrag"),
    ("Message", "Die Nachricht"),
    ("Name", "Der Name"),
    ("Reason", "Der Grund"),
];

/// English message -> German. `{}` stands for a value filled in at run
/// time, in the same order in both.
const CATALOG: &[(&str, &str)] = &[
    // Sessions, sign-up and sign-in
    ("Missing or invalid token", "Du bist nicht (mehr) angemeldet"),
    ("Session ended", "Deine Sitzung wurde beendet"),
    ("Invalid or expired refresh token", "Deine Sitzung ist abgelaufen. Bitte melde dich erneut an."),
    ("No refresh token found", "Deine Sitzung ist abgelaufen. Bitte melde dich erneut an."),
    ("Invalid email, username or password", "E-Mail-Adresse, Nutzername oder Passwort ist falsch"),
    ("Username or email already taken", "Nutzername oder E-Mail-Adresse ist schon vergeben"),
    (
        "You must accept the Terms of Service and Privacy Policy to register",
        "Um dich zu registrieren, musst du den Nutzungsbedingungen und der Datenschutzerklärung zustimmen",
    ),
    ("Invalid or expired verification link", "Der Bestätigungslink ist ungültig oder abgelaufen"),
    ("Invalid or expired reset link", "Der Link zum Zurücksetzen ist ungültig oder abgelaufen"),
    (
        "Please verify your email address first: open the link we emailed you, or request a new one.",
        "Bitte bestätige zuerst deine E-Mail-Adresse: Öffne den Link aus unserer E-Mail oder fordere einen neuen an.",
    ),
    (
        "We suspect someone else has been using this account, so we locked it and emailed you a link \
         to set a new password. Setting one unlocks the account.",
        "Wir vermuten, dass jemand anderes dieses Konto benutzt hat. Deshalb haben wir es gesperrt und dir einen \
         Link geschickt, mit dem du ein neues Passwort setzen kannst. Damit ist das Konto wieder entsperrt.",
    ),
    (
        "We sent a link recently. You can ask for a new one every {} minutes; if it doesn't arrive, write to us.",
        "Wir haben dir vor Kurzem einen Link geschickt. Einen neuen kannst du alle {} Minuten anfordern; wenn keiner ankommt, schreib uns.",
    ),
    ("We sent you a new link.", "Wir haben dir einen neuen Link geschickt."),
    ("This account isn't locked any more; you can log in", "Dieses Konto ist nicht mehr gesperrt; du kannst dich anmelden"),
    ("Rate limit exceeded. Try again in {} seconds.", "Zu viele Anfragen. Versuch es in {} Sekunden noch einmal."),
    // Validation (validation.rs)
    ("This username is not available", "Dieser Nutzername ist nicht verfügbar"),
    ("Username must be between {} and {} characters", "Der Nutzername muss zwischen {} und {} Zeichen lang sein"),
    (
        "Username can only contain letters, numbers, and underscores",
        "Der Nutzername darf nur Buchstaben, Ziffern und Unterstriche enthalten",
    ),
    ("Invalid email address", "Ungültige E-Mail-Adresse"),
    ("Password must be at least {} characters", "Das Passwort muss mindestens {} Zeichen lang sein"),
    ("Password must be {} characters or less", "Das Passwort darf höchstens {} Zeichen lang sein"),
    ("{} must be {} characters or less", "{} darf höchstens {} Zeichen lang sein"),
    ("{} cannot be empty", "{} darf nicht leer sein"),
    ("Unsupported language", "Diese Sprache wird nicht unterstützt"),
    // Profiles, settings, the account
    ("User not found", "Konto nicht gefunden"),
    ("User '{}' not found", "Konto „{}“ nicht gefunden"),
    ("Username is already taken", "Dieser Nutzername ist schon vergeben"),
    ("You can change your username again on {}", "Du kannst deinen Nutzernamen ab dem {} wieder ändern"),
    (
        "Your account is suspended. You can only clear your display name or bio until the suspension ends.",
        "Dein Konto ist gesperrt. Bis die Sperre endet, kannst du nur deinen Anzeigenamen oder deine Bio leeren.",
    ),
    ("Current password is incorrect", "Das aktuelle Passwort ist falsch"),
    ("Invalid current password", "Das aktuelle Passwort ist falsch"),
    ("The password is incorrect", "Das Passwort ist falsch"),
    ("New password must be different from current password", "Das neue Passwort muss sich vom aktuellen unterscheiden"),
    ("Avatar field is required", "Bitte wähle ein Profilbild aus"),
    ("Avatar file is empty", "Die Bilddatei ist leer"),
    ("Avatar must be under 5MB", "Das Profilbild muss kleiner als 5 MB sein"),
    ("Search query cannot be empty", "Gib einen Suchbegriff ein"),
    ("Search query too long", "Der Suchbegriff ist zu lang"),
    ("This account is private", "Dieses Konto ist privat"),
    ("You can't follow yourself", "Du kannst dir nicht selbst folgen"),
    ("Cannot follow this user", "Diesem Konto kannst du nicht folgen"),
    ("Cannot accept this request", "Diese Anfrage kannst du nicht annehmen"),
    ("No pending request from this user", "Von diesem Konto liegt keine offene Anfrage vor"),
    ("You can't block yourself", "Du kannst dich nicht selbst blockieren"),
    // Posts, comments, likes, uploads
    ("Post not found", "Beitrag nicht gefunden"),
    ("Not found", "Nicht gefunden"),
    ("You can only edit your own posts", "Du kannst nur deine eigenen Beiträge bearbeiten"),
    ("You can only delete your own posts", "Du kannst nur deine eigenen Beiträge löschen"),
    ("Cannot like this post", "Diesen Beitrag kannst du nicht liken"),
    ("Cannot comment on this post", "Diesen Beitrag kannst du nicht kommentieren"),
    ("Comment not found", "Kommentar nicht gefunden"),
    ("Parent comment not found on this post", "Den Kommentar, auf den du antwortest, gibt es bei diesem Beitrag nicht"),
    ("You can only edit your own comments", "Du kannst nur deine eigenen Kommentare bearbeiten"),
    (
        "You can only delete your own comments or comments on your posts",
        "Du kannst nur deine eigenen Kommentare löschen oder Kommentare unter deinen Beiträgen",
    ),
    ("Cannot like this comment", "Diesen Kommentar kannst du nicht liken"),
    ("Image field is required", "Bitte wähle ein Bild aus"),
    ("Image file is empty", "Die Bilddatei ist leer"),
    ("Image must be under 20MB", "Das Bild muss kleiner als 20 MB sein"),
    ("Unsupported image type: {}. Allowed: JPEG, PNG, WebP", "Dieser Bildtyp wird nicht unterstützt: {}. Erlaubt sind JPEG, PNG und WebP"),
    ("Image processing failed: {}", "Das Bild konnte nicht verarbeitet werden: {}"),
    ("Failed to read image: {}", "Das Bild konnte nicht gelesen werden: {}"),
    ("Failed to read caption: {}", "Die Bildunterschrift konnte nicht gelesen werden: {}"),
    ("Invalid multipart data: {}", "Ungültige Formulardaten: {}"),
    // Chats
    ("You cannot message yourself", "Du kannst dir nicht selbst schreiben"),
    ("You can only message users who follow you back.", "Du kannst nur Leuten schreiben, die dir zurückfolgen."),
    ("Access denied to this conversation", "Kein Zugriff auf diesen Chat"),
    ("Message not found", "Nachricht nicht gefunden"),
    ("You can only edit your own messages", "Du kannst nur deine eigenen Nachrichten bearbeiten"),
    ("You can only delete your own messages", "Du kannst nur deine eigenen Nachrichten löschen"),
    ("Reply target not found in this conversation", "Die Nachricht, auf die du antwortest, gibt es in diesem Chat nicht"),
    ("Invalid reaction", "Ungültige Reaktion"),
    // Reports, objections, notices, rights claims, feedback
    ("Invalid reason", "Ungültiger Grund"),
    ("Details must be under 1000 characters", "Die Angaben müssen kürzer als 1000 Zeichen sein"),
    ("You already reported this. It's waiting for review.", "Du hast das schon gemeldet. Es wartet auf Prüfung."),
    (
        "You've reached today's limit for reports. If something can't wait, write to kontakt@klarsocial.eu.",
        "Du hast für heute die Höchstzahl an Meldungen erreicht. Wenn etwas nicht warten kann, schreib an kontakt@klarsocial.eu.",
    ),
    ("Decision not found", "Entscheidung nicht gefunden"),
    ("Please explain your objection in at least {} characters", "Bitte begründe deinen Widerspruch mit mindestens {} Zeichen"),
    ("Objection must be under {} characters", "Der Widerspruch muss kürzer als {} Zeichen sein"),
    (
        "This decision can't be objected to (not found, already objected, lifted, replaced by a later decision, or older than six months)",
        "Gegen diese Entscheidung ist kein Widerspruch möglich (nicht gefunden, schon widersprochen, aufgehoben, durch eine spätere Entscheidung ersetzt oder älter als sechs Monate)",
    ),
    ("The note must be under {} characters", "Die Notiz muss kürzer als {} Zeichen sein"),
    ("The reported content no longer exists", "Den gemeldeten Inhalt gibt es nicht mehr"),
    (
        "This report can't be checked again (not dismissed, already re-checked, or older than six months)",
        "Diese Meldung kann nicht erneut geprüft werden (nicht abgelehnt, schon einmal erneut geprüft oder älter als sechs Monate)",
    ),
    ("You already have a report on this waiting for review.", "Du hast dazu schon eine Meldung, die auf Prüfung wartet."),
    (
        "Your account is suspended until {}. You can still read, object to the decision, export your data or delete your account.",
        "Dein Konto ist bis {} gesperrt. Du kannst weiterhin lesen, der Entscheidung widersprechen, deine Daten exportieren oder dein Konto löschen.",
    ),
    (
        "Your account is permanently suspended. You can still read, object to the decision, export your data or delete your account.",
        "Dein Konto ist dauerhaft gesperrt. Du kannst weiterhin lesen, der Entscheidung widersprechen, deine Daten exportieren oder dein Konto löschen.",
    ),
    ("Notice not found", "Meldung nicht gefunden"),
    ("Your name and email address are required", "Dein Name und deine E-Mail-Adresse sind erforderlich"),
    ("The good-faith statement is required", "Die Bestätigung, nach bestem Wissen zu handeln, ist erforderlich"),
    ("Name must be {} characters or less", "Der Name darf höchstens {} Zeichen lang sein"),
    ("We couldn't find that content on Klar (any more)", "Diesen Inhalt haben wir auf Klar nicht (mehr) gefunden"),
    (
        "The link must point to a post, a comment or a profile on Klar (…/posts/…, …/users/…)",
        "Der Link muss auf einen Beitrag, einen Kommentar oder ein Profil auf Klar zeigen (…/posts/…, …/users/…)",
    ),
    ("Invalid claim type", "Ungültige Art der Meldung"),
    ("The link must point to a post on Klar (…/posts/…)", "Der Link muss auf einen Beitrag auf Klar zeigen (…/posts/…)"),
    ("That post doesn't exist (any more)", "Diesen Beitrag gibt es nicht (mehr)"),
    ("Claim not found", "Meldung nicht gefunden"),
    ("No evidence was requested for this claim", "Zu dieser Meldung wurden keine Nachweise angefordert"),
    ("Please write a little more", "Bitte schreib etwas mehr"),
    ("Invalid category", "Ungültige Kategorie"),
    ("Feedback must be under {} characters", "Das Feedback muss kürzer als {} Zeichen sein"),
    ("At most {} screenshots", "Höchstens {} Screenshots"),
    ("Each screenshot must be under 10 MB", "Jeder Screenshot muss kleiner als 10 MB sein"),
    ("Screenshots must be JPEG, PNG or WebP", "Screenshots müssen JPEG-, PNG- oder WebP-Bilder sein"),
    ("Screenshot could not be read: {}", "Der Screenshot konnte nicht gelesen werden: {}"),
    ("Failed to read screenshot: {}", "Der Screenshot konnte nicht gelesen werden: {}"),
    (
        "That's a lot of feedback for one day — thank you! Please try again tomorrow.",
        "Das ist viel Feedback für einen Tag – danke! Bitte versuch es morgen wieder.",
    ),
    // Server errors
    ("Database error", "Ein Fehler ist aufgetreten. Bitte versuch es noch einmal."),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negotiates_like_the_frontend() {
        assert_eq!(Lang::negotiate(None), Lang::En);
        assert_eq!(Lang::negotiate(Some("de-AT,de;q=0.9,en;q=0.8")), Lang::De);
        assert_eq!(Lang::negotiate(Some("fr-FR,fr;q=0.9,de;q=0.5,en;q=0.4")), Lang::De);
        assert_eq!(Lang::negotiate(Some("en-US,de;q=0.9")), Lang::En);
        assert_eq!(Lang::negotiate(Some("en;q=0.5,de")), Lang::De);
        assert_eq!(Lang::negotiate(Some("de;q=0,en")), Lang::En);
        assert_eq!(Lang::negotiate(Some("fr")), Lang::En);
        assert_eq!(Lang::negotiate(Some("DE")), Lang::De);
    }

    #[test]
    fn an_email_follows_the_account_then_the_fallback() {
        assert_eq!(for_account(Some("en"), Lang::De), Lang::En);
        assert_eq!(for_account(Some("de"), Lang::En), Lang::De);
        assert_eq!(for_account(None, Lang::De), Lang::De);
        assert_eq!(for_account(Some("fr"), Lang::En), Lang::En);
    }

    #[test]
    fn translates_exact_messages_and_templates() {
        assert_eq!(tr_in(Lang::De, "Post not found"), "Beitrag nicht gefunden");
        assert_eq!(tr_in(Lang::En, "Post not found"), "Post not found");
        assert_eq!(tr_in(Lang::De, "User 'bob' not found"), "Konto „bob“ nicht gefunden");
        assert_eq!(
            tr_in(Lang::De, "Username must be between 3 and 30 characters"),
            "Der Nutzername muss zwischen 3 und 30 Zeichen lang sein",
        );
        // The specific template wins over the generic one, and field names
        // in the generic ones are translated.
        assert_eq!(tr_in(Lang::De, "Password must be 128 characters or less"), "Das Passwort darf höchstens 128 Zeichen lang sein");
        assert_eq!(tr_in(Lang::De, "Caption must be 2000 characters or less"), "Die Bildunterschrift darf höchstens 2000 Zeichen lang sein");
        assert_eq!(tr_in(Lang::De, "Comment cannot be empty"), "Der Kommentar darf nicht leer sein");
        // Not in the catalog: stays English.
        assert_eq!(tr_in(Lang::De, "Lock not found"), "Lock not found");
    }

    #[test]
    fn templates_need_the_whole_shape() {
        assert_eq!(match_template("At most {} screenshots", "At most 3 screenshots"), Some(vec!["3"]));
        assert_eq!(match_template("At most {} screenshots", "At most screenshots"), None);
        assert_eq!(match_template("{} cannot be empty", "Search query cannot be empty!"), None);
        assert_eq!(
            match_template("Username must be between {} and {} characters", "Username must be between 3 and 30 characters"),
            Some(vec!["3", "30"]),
        );
    }

    /// Every catalog entry and field name still occurs in the source, so a
    /// reworded message can't silently lose its translation.
    #[test]
    fn every_entry_matches_a_message_in_the_code() {
        let mut source = String::new();
        let mut dirs = vec![std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/src"))];
        while let Some(dir) = dirs.pop() {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    dirs.push(path);
                } else if path.extension().is_some_and(|e| e == "rs") && !path.ends_with("i18n.rs") {
                    source.push_str(&std::fs::read_to_string(&path).unwrap());
                }
            }
        }
        // Long messages are split over lines with a trailing backslash in
        // the code; join them the way the compiler does.
        let joined = regex_free_unescape(&source);
        for (en, _) in CATALOG {
            assert!(joined.contains(en), "catalog entry not found in the code: {en:?}");
        }
        for (en, _) in FIELD_NAMES {
            assert!(joined.contains(&format!("\"{en}\"")), "field name not found in the code: {en:?}");
        }
    }

    /// Joins `\` + newline + indentation inside string literals, as Rust
    /// does, so multi-line messages read as one line.
    fn regex_free_unescape(source: &str) -> String {
        let mut out = String::with_capacity(source.len());
        let mut chars = source.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\\' && chars.peek() == Some(&'\n') {
                chars.next();
                while chars.peek().is_some_and(|c| c.is_whitespace()) {
                    chars.next();
                }
            } else {
                out.push(c);
            }
        }
        out
    }
}
