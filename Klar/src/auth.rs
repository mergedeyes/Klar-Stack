/// JWT authentication — access tokens, refresh tokens, and the AuthUser extractor.

use axum::{
    extract::FromRequestParts,
    http::{header, request::Parts},
};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Sha256, Digest};
use uuid::Uuid;
use crate::errors::AppError;

/// JWT claims — stored inside the access token
#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: Uuid,
    pub exp: usize,
}

/// Create a short-lived access token (15 minutes)
pub fn create_access_token(user_id: Uuid, secret: &str) -> Result<String, jsonwebtoken::errors::Error> {
    let expiration = chrono::Utc::now()
        .checked_add_signed(chrono::Duration::minutes(15))
        .expect("valid timestamp")
        .timestamp() as usize;

    let claims = Claims {
        sub: user_id,
        exp: expiration,
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
}

/// Validate an access token
pub fn validate_token(token: &str, secret: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
    let token_data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )?;

    Ok(token_data.claims)
}

/// Generate a random refresh token (64 hex chars = 32 bytes entropy)
pub fn generate_refresh_token() -> String {
    let bytes: [u8; 32] = rand::rng().random();
    hex::encode(bytes)
}

/// Hash a refresh token for storage
pub fn hash_refresh_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}

/// Extractor for required auth — rejects with 401 if no valid token
#[derive(Debug)]
pub struct AuthUser {
    pub user_id: Uuid,
}

impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        // Hier rufen wir deinen Helper auf, der zuerst ins Cookie und dann in den Header schaut
        if let Some(user_id) = extract_user_id(parts) {
            Ok(AuthUser { user_id })
        } else {
            Err(AppError::unauthorized("Missing or invalid token"))
        }
    }
}

/// Extractor for optional auth — never fails, yields None if token is absent or invalid.
/// Use this on endpoints that work for both authenticated and unauthenticated users.
#[derive(Debug)]
pub struct OptionalAuthUser {
    pub user_id: Option<Uuid>,
}

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

/// Shared helper — pulls the token from the Cookie, falling back to Authorization header.
fn extract_user_id(parts: &Parts) -> Option<Uuid> {
    let secret = std::env::var("JWT_SECRET").ok()?;
    let mut token_str = None;

    // 1. Try to extract from httpOnly Cookie
    if let Some(cookie_header) = parts.headers.get(header::COOKIE).and_then(|v| v.to_str().ok()) {
        if let Some(token) = cookie_header
            .split(';')
            .map(|s| s.trim())
            .find(|s| s.starts_with("klar_access_token="))
            .and_then(|s| s.strip_prefix("klar_access_token="))
        {
            token_str = Some(token);
        }
    }

    // 2. Fallback to Authorization Bearer header
    if token_str.is_none() {
        if let Some(auth_header) = parts.headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()) {
            token_str = auth_header.strip_prefix("Bearer ");
        }
    }

    let token = token_str?;
    validate_token(token, &secret).ok().map(|claims| claims.sub)
}