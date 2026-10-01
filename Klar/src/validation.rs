//! Input validation shared by the handlers, so each rule lives in one
//! place and the limits match what the frontend forms enforce.

use crate::errors::AppError;

pub const USERNAME_MIN: usize = 3;
pub const USERNAME_MAX: usize = 30;
pub const PASSWORD_MIN: usize = 8;
/// Upper bound so a multi-megabyte "password" can't tie up Argon2.
pub const PASSWORD_MAX: usize = 128;
pub const EMAIL_MAX: usize = 255; // users.email is VARCHAR(255)
pub const DISPLAY_NAME_MAX: usize = 50; // users.display_name is VARCHAR(50)
pub const BIO_MAX: usize = 500;
pub const CAPTION_MAX: usize = 2000;
pub const COMMENT_MAX: usize = 2000;
pub const MESSAGE_MAX: usize = 2000;

/// Usernames that collide with fixed routes (/users/me, /users/search).
/// Never allowed, not even for official accounts. Compared
/// case-insensitively.
const ROUTE_USERNAMES: &[&str] = &["me", "search"];

/// Usernames that could be used to impersonate staff. Only official
/// accounts may have them, and only through an admin rename
/// (handlers/official_accounts.rs). Compared case-insensitively.
const STAFF_USERNAMES: &[&str] = &[
    "admin", "administrator", "mod", "moderator", "staff",
    "support", "help", "official", "system", "root", "klar", "klarsocial",
];

/// Trims and validates a new username: 3-30 characters, ASCII letters,
/// digits and underscores only (the same rule as the registration form),
/// and not reserved. Returns the trimmed name with its case preserved.
pub fn validate_username(raw: &str) -> Result<String, AppError> {
    let username = validate_official_username(raw)?;
    if STAFF_USERNAMES.contains(&username.to_ascii_lowercase().as_str()) {
        return Err(AppError::bad_request("This username is not available"));
    }
    Ok(username)
}

/// validate_username without the staff names, for the admin rename of an
/// official account (e.g. kontakt@klarsocial.eu becoming "Klar").
pub fn validate_official_username(raw: &str) -> Result<String, AppError> {
    let username = raw.trim();
    let len = username.chars().count();

    if !(USERNAME_MIN..=USERNAME_MAX).contains(&len) {
        return Err(AppError::bad_request(format!(
            "Username must be between {} and {} characters",
            USERNAME_MIN, USERNAME_MAX
        )));
    }
    if !username.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(AppError::bad_request(
            "Username can only contain letters, numbers, and underscores",
        ));
    }
    if ROUTE_USERNAMES.contains(&username.to_ascii_lowercase().as_str()) {
        return Err(AppError::bad_request("This username is not available"));
    }

    Ok(username.to_string())
}

/// Canonical form of an email for storage and lookup: trimmed and
/// lowercased. Emails are unique and matched case-insensitively
/// (migration 20260929000000), so "Foo@x.de" and "foo@x.de" are one
/// account. Lowercasing the local part is technically stricter than
/// RFC 5321, but every mainstream provider treats it case-insensitively.
pub fn normalize_email(raw: &str) -> String {
    raw.trim().to_lowercase()
}

/// normalize_email plus a basic shape check, for addresses being stored
/// (registration). Real validation is the verification email itself.
pub fn validate_new_email(raw: &str) -> Result<String, AppError> {
    let email = normalize_email(raw);
    let valid_shape = match email.split_once('@') {
        Some((local, domain)) => {
            !local.is_empty() && domain.contains('.') && !domain.starts_with('.')
                && !domain.ends_with('.') && !email.chars().any(char::is_whitespace)
        }
        None => false,
    };
    if !valid_shape || email.chars().count() > EMAIL_MAX {
        return Err(AppError::bad_request("Invalid email address"));
    }
    Ok(email)
}

pub fn validate_password(password: &str) -> Result<(), AppError> {
    let len = password.chars().count();
    if len < PASSWORD_MIN {
        return Err(AppError::bad_request(format!(
            "Password must be at least {} characters",
            PASSWORD_MIN
        )));
    }
    if len > PASSWORD_MAX {
        return Err(AppError::bad_request(format!(
            "Password must be {} characters or less",
            PASSWORD_MAX
        )));
    }
    Ok(())
}

/// Rejects `value` if it's longer than `max` characters (not bytes -- an
/// umlaut counts as one, matching the frontend's maxLength).
pub fn check_max_len(value: &str, field: &str, max: usize) -> Result<(), AppError> {
    if value.chars().count() > max {
        return Err(AppError::bad_request(format!(
            "{} must be {} characters or less",
            field, max
        )));
    }
    Ok(())
}

/// Trims `value` and requires it to be non-empty and at most `max`
/// characters. Returns the trimmed text.
pub fn required_text<'a>(value: &'a str, field: &str, max: usize) -> Result<&'a str, AppError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(AppError::bad_request(format!("{} cannot be empty", field)));
    }
    check_max_len(trimmed, field, max)?;
    Ok(trimmed)
}

/// Escapes LIKE/ILIKE wildcards so user input matches literally (the
/// default escape character in Postgres is the backslash).
pub fn escape_like(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for c in input.chars() {
        if matches!(c, '\\' | '%' | '_') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Clamps a client-supplied page size to 1..=max (a negative LIMIT is a
/// Postgres error, i.e. a 500).
pub fn page_limit(requested: Option<i64>, default: i64, max: i64) -> i64 {
    requested.unwrap_or(default).clamp(1, max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn username_rules() {
        assert_eq!(validate_username("  John_Doe1 ").unwrap(), "John_Doe1");
        assert!(validate_username("   ").is_err());
        assert!(validate_username("ab").is_err());
        assert!(validate_username(&"a".repeat(31)).is_err());
        assert!(validate_username(&"a".repeat(30)).is_ok());
        assert!(validate_username("john.doe").is_err());
        assert!(validate_username("jöhn").is_err());
        assert!(validate_username("a/b").is_err());
        assert!(validate_username("ME").is_err());
        assert!(validate_username("Search").is_err());
        assert!(validate_username("admin").is_err());
        assert!(validate_username("meadow").is_ok());
        // Official accounts may take staff names, but never route names.
        assert_eq!(validate_official_username(" Klar ").unwrap(), "Klar");
        assert!(validate_username("Klar").is_err());
        assert!(validate_official_username("me").is_err());
        assert!(validate_official_username("SEARCH").is_err());
        assert!(validate_official_username("klar.social").is_err());
    }

    #[test]
    fn email_rules() {
        assert_eq!(validate_new_email("  Foo@Example.DE ").unwrap(), "foo@example.de");
        assert!(validate_new_email("foo").is_err());
        assert!(validate_new_email("@example.de").is_err());
        assert!(validate_new_email("foo@localhost").is_err());
        assert!(validate_new_email("foo@.de").is_err());
        assert!(validate_new_email("fo o@example.de").is_err());
        let long = format!("{}@example.de", "a".repeat(250));
        assert!(validate_new_email(&long).is_err());
    }

    #[test]
    fn password_rules() {
        assert!(validate_password("1234567").is_err());
        assert!(validate_password("12345678").is_ok());
        assert!(validate_password(&"x".repeat(129)).is_err());
    }

    #[test]
    fn lengths_count_chars_not_bytes() {
        assert!(check_max_len(&"ä".repeat(50), "Display name", 50).is_ok());
        assert!(check_max_len(&"ä".repeat(51), "Display name", 50).is_err());
        assert_eq!(required_text("  hi  ", "Message", 10).unwrap(), "hi");
        assert!(required_text(" \n ", "Message", 10).is_err());
    }

    #[test]
    fn like_escaping() {
        assert_eq!(escape_like(r"50%_off\"), r"50\%\_off\\");
    }

    #[test]
    fn limits_are_clamped() {
        assert_eq!(page_limit(None, 20, 50), 20);
        assert_eq!(page_limit(Some(-5), 20, 50), 1);
        assert_eq!(page_limit(Some(0), 20, 50), 1);
        assert_eq!(page_limit(Some(500), 20, 50), 50);
    }
}
