/// JWT authentication: access tokens, refresh tokens, and the AuthUser extractor.

use axum::{
    extract::FromRequestParts, // trait that lets a type be built directly from request parts (headers, uri, etc.)
    http::{header, request::Parts}, // header = well-known header name constants; Parts = the request minus its body
};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation}; // JWT encode/decode primitives
use rand::Rng; // trait providing .random() for generating random bytes
use serde::{Deserialize, Serialize}; // derive macros to (de)serialize the JWT claims to/from JSON
use sha2::{Sha256, Digest}; // SHA-256 hashing, used to hash refresh tokens before storing them
use uuid::Uuid;
use crate::errors::AppError;

/// JWT claims, stored inside the access token
#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: Uuid,   // "subject", the authenticated user's ID (standard JWT claim name)
    pub exp: usize,  // "expiration", Unix timestamp (seconds) after which the token is invalid (standard JWT claim name)
}

/// Create a short-lived access token (15 minutes)
pub fn create_access_token(user_id: Uuid, secret: &str) -> Result<String, jsonwebtoken::errors::Error> {
    // Compute the expiration timestamp: now plus 15 minutes, as a Unix timestamp (seconds).
    let expiration = chrono::Utc::now()
        .checked_add_signed(chrono::Duration::minutes(15))
        .expect("valid timestamp") // adding 15 minutes to "now" cannot overflow chrono's internal date range, so this expect is safe
        .timestamp() as usize; // exp is usize and timestamp() is i64; on this deployment (64 bit Linux) usize is also 64 bits, so this cast is lossless for a very long time. On a 32 bit target it would silently truncate instead of panicking, since "as" casts do not panic.

    let claims = Claims {
        sub: user_id,
        exp: expiration,
    };

    // Sign the claims into a JWT using HMAC with the default header (HS256) and the given secret.
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
}

/// Validate an access token
pub fn validate_token(token: &str, secret: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
    // Decode and verify the JWT signature and its claims (including expiry) against the given secret.
    // Returns an Err if the signature is invalid, the token is malformed, or it has expired.
    let token_data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )?;

    Ok(token_data.claims)
}

/// Generate a random refresh token (64 hex chars = 32 bytes entropy)
pub fn generate_refresh_token() -> String {
    // Generate 32 cryptographically random bytes (256 bits of entropy)...
    let bytes: [u8; 32] = rand::rng().random();
    // ...and hex-encode them into a 64-character string. This is the plaintext token
    // that gets sent to the client; only its hash (see below) is stored server-side.
    hex::encode(bytes)
}

/// Hash a refresh token for storage
pub fn hash_refresh_token(token: &str) -> String {
    // Refresh tokens are stored as SHA-256 hashes rather than plaintext, so that a
    // database leak alone doesn't expose usable tokens (same idea as password hashing,
    // though SHA-256 is fine here since the input is already high-entropy random data,
    // not a low-entropy human-chosen secret).
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}

/// Extractor for required auth: rejects with 401 if no valid token
#[derive(Debug)]
pub struct AuthUser {
    pub user_id: Uuid,
}

// Implementing FromRequestParts lets Axum inject AuthUser directly as a handler
// argument. Axum calls this automatically before the handler runs, and if it
// returns Err, the handler is never called and the rejection is returned instead.
impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
{
    type Rejection = AppError; // what gets returned to the client if extraction fails

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        // Hier rufen wir deinen Helper auf, der zuerst ins Cookie und dann in den Header schaut
        // (Here we call the shared helper, which checks the cookie first, then the header.)
        if let Some(user_id) = extract_user_id(parts) {
            Ok(AuthUser { user_id })
        } else {
            // No valid token found anywhere (cookie, header, or query param), so reject with 401.
            Err(AppError::unauthorized("Missing or invalid token"))
        }
    }
}

/// Extractor for optional auth: never fails, yields None if token is absent or invalid.
/// Use this on endpoints that work for both authenticated and unauthenticated users.
#[derive(Debug)]
pub struct OptionalAuthUser {
    pub user_id: Option<Uuid>,
}

// Same idea as AuthUser, but this extractor can never fail (Rejection = Infallible).
// Instead of rejecting the request, a missing or invalid token just results in
// user_id: None, letting the handler itself decide how to treat anonymous requests.
impl<S> FromRequestParts<S> for OptionalAuthUser
where
    S: Send + Sync,
{
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(OptionalAuthUser {
            user_id: extract_user_id(parts),
        })
    }
}

/// Shared helper: pulls the token from the Cookie, falling back to the
/// Authorization header, falling back to a "token" query parameter.
///
/// The query-param fallback exists specifically for EventSource (used by
/// the /notifications/stream SSE endpoint): browsers' EventSource API can't
/// set custom headers at all, and with klarsocial.eu/.de being genuinely
/// cross-site, third-party cookie blocking means EventSource's cookie may
/// never arrive either. A query param is the only channel left it can use.
fn extract_user_id(parts: &Parts) -> Option<Uuid> {
    // Bail out immediately (returning None) if JWT_SECRET isn't set, since without it
    // no token could ever be validated anyway.
    let secret = std::env::var("JWT_SECRET").ok()?;
    let mut token_str = None;

    // 1. Try to extract from httpOnly Cookie
    if let Some(cookie_header) = parts.headers.get(header::COOKIE).and_then(|v| v.to_str().ok()) {
        if let Some(token) = cookie_header
            .split(';')                                        // cookies are separated by "; "
            .map(|s| s.trim())                                  // strip leading/trailing whitespace from each pair
            .find(|s| s.starts_with("klar_access_token="))      // locate our specific cookie by name
            .and_then(|s| s.strip_prefix("klar_access_token=")) // drop the "name=" prefix, leaving just the value
        {
            token_str = Some(token);
        }
    }

    // 2. Fallback to Authorization Bearer header
    if token_str.is_none() {
        if let Some(auth_header) = parts.headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()) {
            // Expects a header of the form "Authorization: Bearer <token>"; strips the "Bearer " prefix.
            // If the header doesn't start with "Bearer ", strip_prefix returns None and token_str stays None.
            token_str = auth_header.strip_prefix("Bearer ");
        }
    }

    // 3. Fallback to a "token" query parameter (EventSource can't set headers,
    // and may not reliably receive the cookie cross-site either). JWTs are
    // base64url-encoded (RFC 4648 par. 5), which is already URL-safe, so no
    // percent-decoding is needed for this specific token format.
    if token_str.is_none() {
        if let Some(query) = parts.uri.query() {
            // Manually scan "key=value&key=value" pairs for one named "token", avoiding
            // the need to pull in a query-string parsing crate for this single use case.
            token_str = query.split('&').find_map(|pair| {
                let mut kv = pair.splitn(2, '='); // split into at most 2 parts, in case the value itself contains "="
                let key = kv.next()?;
                let val = kv.next()?;
                if key == "token" { Some(val) } else { None }
            });
        }
    }

    // At this point, token_str is either the token found via one of the three methods
    // above, or None if all of them failed, in which case we return None here too.
    let token = token_str?;
    // Validate the token's signature and expiry; on success, extract the user ID (the sub claim).
    // .ok() converts a validation error into None (rather than propagating the error type),
    // since both extractors above just want a plain Option<Uuid>.
    validate_token(token, &secret).ok().map(|claims| claims.sub)
}
