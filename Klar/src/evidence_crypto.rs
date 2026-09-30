//! Encryption of evidence files at rest.
//!
//! Every file written to the evidence storage zone (storage.rs,
//! EvidenceStorage) is encrypted with AES-256-GCM before upload and
//! decrypted when an admin views it. A leaked zone password, a copy of the
//! bucket, or someone browsing it at the provider then yields nothing
//! readable. The key only exists in the backend's environment (and the
//! operator's password manager), never in the database or its backups.
//!
//! EVIDENCE_ENCRYPTION_KEY is any high-entropy secret of at least 32
//! characters, e.g. a random password generated in a password manager.
//! HKDF-SHA256 turns it into the 256-bit key. Losing it makes every
//! preserved file unreadable, so it must be kept somewhere safe besides the
//! deployment.
//!
//! File format: "KLE1" | key fingerprint (8 bytes) | nonce (12 bytes) |
//! ciphertext + tag. The fingerprint says which key a file needs, so a
//! wrong or rotated key fails with a clear error instead of a generic
//! authentication failure. The storage key is the associated data: a file
//! moved to another key within the zone fails to decrypt, so files can't be
//! swapped between records unnoticed.

use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM, NONCE_LEN};
use ring::rand::{SecureRandom, SystemRandom};
use ring::{digest, hkdf};

use crate::errors::AppError;

const MAGIC: &[u8; 4] = b"KLE1";
const FINGERPRINT_LEN: usize = 8;
const HEADER_LEN: usize = MAGIC.len() + FINGERPRINT_LEN + NONCE_LEN;
pub const MIN_SECRET_LEN: usize = 32;

/// Fixed salt: the secret is already high-entropy, so HKDF's extract step
/// only needs domain separation, not a random salt.
const HKDF_SALT: &[u8] = b"klar evidence encryption";
const HKDF_INFO: &[u8] = b"aes-256-gcm v1";

#[derive(Clone)]
pub struct EvidenceCipher {
    key: std::sync::Arc<LessSafeKey>,
    fingerprint: [u8; FINGERPRINT_LEN],
}

/// HKDF's output length type for a 256-bit key.
struct KeyLen;

impl hkdf::KeyType for KeyLen {
    fn len(&self) -> usize {
        32
    }
}

impl EvidenceCipher {
    pub fn from_secret(secret: &str) -> Result<Self, String> {
        let secret = secret.trim();
        if secret.chars().count() < MIN_SECRET_LEN {
            return Err(format!("EVIDENCE_ENCRYPTION_KEY must be at least {MIN_SECRET_LEN} characters"));
        }

        let prk = hkdf::Salt::new(hkdf::HKDF_SHA256, HKDF_SALT).extract(secret.as_bytes());
        let mut key_bytes = [0u8; 32];
        prk.expand(&[HKDF_INFO], KeyLen)
            .and_then(|okm| okm.fill(&mut key_bytes))
            .map_err(|_| "Key derivation failed".to_string())?;

        let mut fingerprint = [0u8; FINGERPRINT_LEN];
        fingerprint.copy_from_slice(&digest::digest(&digest::SHA256, &key_bytes).as_ref()[..FINGERPRINT_LEN]);

        let key = UnboundKey::new(&AES_256_GCM, &key_bytes).map_err(|_| "Invalid key".to_string())?;
        Ok(Self { key: std::sync::Arc::new(LessSafeKey::new(key)), fingerprint })
    }

    /// Short hex id of the key, safe to log: it identifies the key without
    /// revealing it.
    pub fn fingerprint_hex(&self) -> String {
        hex::encode(self.fingerprint)
    }

    pub fn encrypt(&self, storage_key: &str, plaintext: &[u8]) -> Result<Vec<u8>, AppError> {
        let mut nonce = [0u8; NONCE_LEN];
        SystemRandom::new()
            .fill(&mut nonce)
            .map_err(|_| AppError::internal("Failed to generate nonce"))?;

        let mut out = Vec::with_capacity(HEADER_LEN + plaintext.len() + AES_256_GCM.tag_len());
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&self.fingerprint);
        out.extend_from_slice(&nonce);

        let mut body = plaintext.to_vec();
        self.key
            .seal_in_place_append_tag(Nonce::assume_unique_for_key(nonce), Aad::from(storage_key.as_bytes()), &mut body)
            .map_err(|_| AppError::internal("Encryption failed"))?;
        out.extend_from_slice(&body);
        Ok(out)
    }

    pub fn decrypt(&self, storage_key: &str, data: &[u8]) -> Result<Vec<u8>, AppError> {
        if data.len() < HEADER_LEN || &data[..MAGIC.len()] != MAGIC {
            // Every file is written encrypted; anything else wasn't written
            // by us and is refused rather than served.
            tracing::error!("Evidence file {} is not in the encrypted format", storage_key);
            return Err(AppError::internal("Evidence file is not encrypted"));
        }
        let fingerprint = &data[MAGIC.len()..MAGIC.len() + FINGERPRINT_LEN];
        if fingerprint != self.fingerprint {
            tracing::error!(
                "Evidence file {} was encrypted with key {}, configured key is {}",
                storage_key, hex::encode(fingerprint), self.fingerprint_hex()
            );
            return Err(AppError::internal("Evidence file was encrypted with a different key"));
        }

        let mut nonce = [0u8; NONCE_LEN];
        nonce.copy_from_slice(&data[MAGIC.len() + FINGERPRINT_LEN..HEADER_LEN]);
        let mut body = data[HEADER_LEN..].to_vec();
        let plaintext = self
            .key
            .open_in_place(Nonce::assume_unique_for_key(nonce), Aad::from(storage_key.as_bytes()), &mut body)
            .map_err(|_| {
                tracing::error!("Evidence file {} failed authentication (tampered or moved)", storage_key);
                AppError::internal("Evidence file failed integrity check")
            })?;
        Ok(plaintext.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "correct-horse-battery-staple-0123456789";

    #[test]
    fn round_trip() {
        let c = EvidenceCipher::from_secret(SECRET).unwrap();
        let data = b"image bytes".to_vec();
        let sealed = c.encrypt("a/b.webp", &data).unwrap();
        assert_ne!(&sealed[HEADER_LEN..HEADER_LEN + data.len()], &data[..]);
        assert_eq!(c.decrypt("a/b.webp", &sealed).unwrap(), data);
    }

    #[test]
    fn nonces_differ() {
        let c = EvidenceCipher::from_secret(SECRET).unwrap();
        assert_ne!(c.encrypt("k", b"x").unwrap(), c.encrypt("k", b"x").unwrap());
    }

    #[test]
    fn rejects_wrong_key_tampering_moves_and_plaintext() {
        let c = EvidenceCipher::from_secret(SECRET).unwrap();
        let sealed = c.encrypt("a/b.webp", b"image bytes").unwrap();

        let other = EvidenceCipher::from_secret("another-long-secret-for-testing-0000000").unwrap();
        assert!(other.decrypt("a/b.webp", &sealed).is_err());

        let mut tampered = sealed.clone();
        *tampered.last_mut().unwrap() ^= 1;
        assert!(c.decrypt("a/b.webp", &tampered).is_err());

        assert!(c.decrypt("a/other.webp", &sealed).is_err());
        assert!(c.decrypt("a/b.webp", b"plain image bytes, long enough for a header").is_err());
    }

    #[test]
    fn rejects_short_secret() {
        assert!(EvidenceCipher::from_secret("too short").is_err());
    }

    #[test]
    fn same_secret_same_fingerprint() {
        let a = EvidenceCipher::from_secret(SECRET).unwrap();
        let b = EvidenceCipher::from_secret(&format!("  {SECRET}\n")).unwrap();
        assert_eq!(a.fingerprint_hex(), b.fingerprint_hex());
    }
}
