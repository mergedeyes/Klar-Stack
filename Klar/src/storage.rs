use aws_sdk_s3::{config::Credentials, Client as S3Client};
use aws_sdk_s3::primitives::ByteStream;
use reqwest::Client as HttpClient;
use crate::errors::AppError;
use crate::evidence_crypto::EvidenceCipher;

// ─── DER WRAPPER ─────────────────────────────────────────────────────────────
// Diese Struktur wird im AppState gespeichert. Sie leitet jeden Aufruf
// einfach an den aktiven Provider weiter.
#[derive(Clone)]
pub struct Storage {
    backend: Backend,
    signer: Option<UrlSigner>,
}

#[derive(Clone)]
enum Backend {
    S3(S3Storage),
    Bunny(BunnyStorage),
    Local(LocalStorage),
}

impl Storage {
    /// Initialisiert den Storage basierend auf der .env Variable.
    /// Setze STORAGE_PROVIDER=s3, um Bunny über die S3-kompatible API zu nutzen,
    /// oder STORAGE_PROVIDER=local, um lokal auf die Festplatte zu schreiben
    /// (siehe LocalStorage weiter unten — nur für lokale Entwicklung gedacht,
    /// damit `cargo run` niemals versehentlich echte Prod-Dateien schreibt
    /// oder löscht).
    pub async fn new() -> Self {
        let provider = std::env::var("STORAGE_PROVIDER")
            .unwrap_or_else(|_| "bunny".to_string())
            .to_lowercase();

        let backend = match provider.as_str() {
            "s3" => {
                tracing::info!("Storage Backend: S3 Compatible");
                Backend::S3(S3Storage::new().await)
            }
            "local" => {
                tracing::info!("Storage Backend: Lokale Festplatte (nur Dev)");
                Backend::Local(LocalStorage::new())
            }
            _ => {
                tracing::info!("Storage Backend: Bunny.net REST API");
                Backend::Bunny(BunnyStorage::new())
            }
        };

        // Local files are served by our own /media route, which doesn't
        // check tokens, so signing only applies to the CDN-backed providers.
        let signer = match backend {
            Backend::Local(_) => None,
            _ => UrlSigner::from_env(),
        };

        Self { backend, signer }
    }

    pub async fn save(&self, key: &str, data: &[u8]) -> Result<(), AppError> {
        match &self.backend {
            Backend::S3(s) => s.save(key, data).await,
            Backend::Bunny(b) => b.save(key, data).await,
            Backend::Local(l) => l.save(key, data).await,
        }
    }

    pub async fn get(&self, key: &str) -> Result<Vec<u8>, AppError> {
        match &self.backend {
            Backend::S3(s) => s.get(key).await,
            Backend::Bunny(b) => b.get(key).await,
            Backend::Local(l) => l.get(key).await,
        }
    }

    pub async fn delete(&self, key: &str) -> Result<(), AppError> {
        match &self.backend {
            Backend::S3(s) => s.delete(key).await,
            Backend::Bunny(b) => b.delete(key).await,
            Backend::Local(l) => l.delete(key).await,
        }
    }

    /// The bare, unsigned URL of a file. Only for identifying the file to
    /// the CDN (cache purges); anything sent to a client goes through
    /// media_url(), since unsigned URLs are rejected once token
    /// authentication is on.
    pub fn public_url(&self, key: &str) -> String {
        match &self.backend {
            Backend::S3(s) => s.public_url(key),
            Backend::Bunny(b) => b.public_url(key),
            Backend::Local(l) => l.public_url(key),
        }
    }

    /// The URL a client may load the file from: signed and expiring when
    /// BUNNY_TOKEN_KEY is set, the plain public URL otherwise.
    pub fn media_url(&self, key: &str) -> String {
        let url = self.public_url(key);
        match &self.signer {
            Some(signer) => signer.sign(&url, chrono::Utc::now().timestamp()),
            None => url,
        }
    }

    /// Resolves an optional storage key into a client-facing URL, passing
    /// None through unchanged. Most media/avatar fields are optional (not
    /// every post has media yet, not every user has an avatar), so this
    /// saves every call site from repeating the same Option handling
    /// around media_url(). Used by the ResolveMedia impls in utils.rs.
    pub fn resolve(&self, key: Option<String>) -> Option<String> {
        key.map(|k| self.media_url(&k))
    }
}

// ─── EVIDENCE STORAGE ────────────────────────────────────────────────────────
// Preserved copies of reported content (see evidence.rs) live in their own
// storage zone that has no pull zone, so nothing in it is reachable from
// the internet; the only way to read a file is the admin-only evidence
// endpoint, which logs every access. It gets its own credentials so a
// leaked media key can't reach it either.
//
// Every file is encrypted before upload (evidence_crypto.rs).
//
// Variables (production, S3):
//   EVIDENCE_S3_STORAGE_BUCKET=<evidence storage zone>
//   EVIDENCE_S3_STORAGE_SECRET_ACCESS_KEY=<its password>
//   EVIDENCE_ENCRYPTION_KEY=<random secret, 32+ characters>
//   EVIDENCE_S3_STORAGE_ENDPOINT   (optional, defaults to S3_STORAGE_ENDPOINT)
// Local development (STORAGE_PROVIDER=local) writes to
// EVIDENCE_LOCAL_STORAGE_DIR, default "./evidence", and falls back to a
// fixed development key when EVIDENCE_ENCRYPTION_KEY isn't set.
//
// When it isn't configured (including a missing or invalid key), nothing
// breaks: preserved files stay at their original key and the evidence
// sweeper copies them once it is.
#[derive(Clone)]
pub struct EvidenceStorage {
    backend: Option<(Backend, EvidenceCipher)>,
}

/// Only for STORAGE_PROVIDER=local, so development works without setup.
/// Never used with a real storage zone.
const DEV_EVIDENCE_KEY: &str = "klar-local-development-evidence-key-not-secret";

impl EvidenceStorage {
    pub async fn new() -> Self {
        let provider = std::env::var("STORAGE_PROVIDER")
            .unwrap_or_else(|_| "bunny".to_string())
            .to_lowercase();
        let non_empty = |name: &str| std::env::var(name).ok().filter(|v| !v.trim().is_empty());

        let secret = non_empty("EVIDENCE_ENCRYPTION_KEY");

        let backend = if provider == "local" {
            let root = non_empty("EVIDENCE_LOCAL_STORAGE_DIR").unwrap_or_else(|| "./evidence".to_string());
            Some(Backend::Local(LocalStorage::at(&root, "")))
        } else {
            match (non_empty("EVIDENCE_S3_STORAGE_BUCKET"), non_empty("EVIDENCE_S3_STORAGE_SECRET_ACCESS_KEY")) {
                // Pointing both at the same zone would put evidence behind
                // the public pull zone, which defeats the point.
                (Some(bucket), _) if non_empty("S3_STORAGE_BUCKET").as_deref() == Some(bucket.as_str()) => {
                    tracing::error!("EVIDENCE_S3_STORAGE_BUCKET must not be the media bucket; evidence storage disabled");
                    None
                }
                (Some(bucket), Some(secret)) => {
                    let endpoint = non_empty("EVIDENCE_S3_STORAGE_ENDPOINT")
                        .or_else(|| non_empty("S3_STORAGE_ENDPOINT"))
                        .expect("EVIDENCE_S3_STORAGE_ENDPOINT or S3_STORAGE_ENDPOINT must be set");
                    let region = derive_region_from_endpoint(&endpoint).unwrap_or_else(|| "us-east-1".to_string());
                    Some(Backend::S3(
                        S3Storage::connect(endpoint, bucket.clone(), bucket, secret, region, String::new()).await,
                    ))
                }
                _ => {
                    tracing::error!(
                        "Evidence storage is not configured (EVIDENCE_S3_STORAGE_BUCKET / \
                         EVIDENCE_S3_STORAGE_SECRET_ACCESS_KEY): reported content that gets deleted \
                         keeps its original files until it is"
                    );
                    None
                }
            }
        };

        let cipher = match (&backend, secret) {
            (None, _) => None,
            (Some(_), Some(secret)) => match EvidenceCipher::from_secret(&secret) {
                Ok(cipher) => Some(cipher),
                Err(e) => {
                    tracing::error!("{}; evidence storage disabled", e);
                    None
                }
            },
            (Some(Backend::Local(_)), None) => {
                tracing::warn!("EVIDENCE_ENCRYPTION_KEY not set; using the fixed local development key");
                EvidenceCipher::from_secret(DEV_EVIDENCE_KEY).ok()
            }
            (Some(_), None) => {
                tracing::error!(
                    "EVIDENCE_ENCRYPTION_KEY is not set: evidence storage disabled until it is \
                     (reported content keeps its original files meanwhile)"
                );
                None
            }
        };

        let backend = match (backend, cipher) {
            (Some(backend), Some(cipher)) => {
                tracing::info!("Evidence storage ready (encryption key {})", cipher.fingerprint_hex());
                Some((backend, cipher))
            }
            _ => None,
        };

        Self { backend }
    }

    pub fn is_configured(&self) -> bool {
        self.backend.is_some()
    }

    fn backend(&self) -> Result<&(Backend, EvidenceCipher), AppError> {
        self.backend.as_ref().ok_or_else(|| AppError::internal("Evidence storage is not configured"))
    }

    /// Encrypts, then uploads. The storage key is bound into the
    /// ciphertext, so the file only decrypts under this key.
    pub async fn save(&self, key: &str, data: &[u8]) -> Result<(), AppError> {
        let (backend, cipher) = self.backend()?;
        let sealed = cipher.encrypt(key, data)?;
        match backend {
            Backend::S3(s) => s.save(key, &sealed).await,
            Backend::Bunny(b) => b.save(key, &sealed).await,
            Backend::Local(l) => l.save(key, &sealed).await,
        }
    }

    /// Downloads, then decrypts and verifies.
    pub async fn get(&self, key: &str) -> Result<Vec<u8>, AppError> {
        let (backend, cipher) = self.backend()?;
        let sealed = match backend {
            Backend::S3(s) => s.get(key).await?,
            Backend::Bunny(b) => b.get(key).await?,
            Backend::Local(l) => l.get(key).await?,
        };
        cipher.decrypt(key, &sealed)
    }

    pub async fn delete(&self, key: &str) -> Result<(), AppError> {
        match &self.backend()?.0 {
            Backend::S3(s) => s.delete(key).await,
            Backend::Bunny(b) => b.delete(key).await,
            Backend::Local(l) => l.delete(key).await,
        }
    }
}

// ─── SIGNED MEDIA URLS ───────────────────────────────────────────────────────
// With Bunny's token authentication enabled on the pull zone, the CDN only
// serves a file for a URL carrying a valid, unexpired token. The API only
// hands media URLs to people allowed to see the post, so this bounds how
// long a URL keeps working once it has been copied elsewhere, or after an
// unfollow, a block or a switch to a private account.
//
// Expiry is rounded to fixed buckets instead of "now + TTL": within one
// bucket every request gets the same URL, so browser, next/image and CDN
// caching keep working. A URL is valid until the end of the bucket after
// the one it was issued in, i.e. between one and two buckets.
const URL_BUCKET_SECS: i64 = 6 * 60 * 60;

#[derive(Clone)]
struct UrlSigner {
    key: String,
}

impl UrlSigner {
    /// BUNNY_TOKEN_KEY is the pull zone's token authentication key. Empty
    /// counts as unset (declared empty in the Dockerfile so Bunny lists it).
    fn from_env() -> Option<Self> {
        let key = std::env::var("BUNNY_TOKEN_KEY")
            .ok()
            .filter(|s| !s.trim().is_empty());

        if key.is_none() {
            tracing::warn!("BUNNY_TOKEN_KEY not set -- media URLs are not signed");
        }

        key.map(|key| Self { key })
    }

    /// Bunny's token scheme: token = base64url(sha256(key + path + expires)),
    /// unpadded, appended as ?token=…&expires=….
    fn sign(&self, url: &str, now: i64) -> String {
        use base64::Engine;
        use sha2::{Digest, Sha256};

        let expires = (now.div_euclid(URL_BUCKET_SECS) + 2) * URL_BUCKET_SECS;
        let path = url
            .split_once("://")
            .and_then(|(_, rest)| rest.find('/').map(|i| &rest[i..]))
            .unwrap_or("/");

        let mut hasher = Sha256::new();
        hasher.update(self.key.as_bytes());
        hasher.update(path.as_bytes());
        hasher.update(expires.to_string().as_bytes());
        let token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(hasher.finalize());

        format!("{}?token={}&expires={}", url, token, expires)
    }
}

// ─── CDN CACHE PURGE ─────────────────────────────────────────────────────────
// Deleting a file from the storage zone does not take it offline: the pull
// zone's edge servers keep serving their cached copy until it expires. When
// a file has to disappear *now* (media of a post hidden by moderation), its
// URL must also be purged. That needs the account-wide Bunny API key --
// the storage zone password can't purge -- so it's optional: without
// BUNNY_API_KEY purges are skipped with a warning (local dev, S3 elsewhere).
#[derive(Clone)]
pub struct CdnPurger {
    client: HttpClient,
    api_key: Option<String>,
}

impl CdnPurger {
    pub fn new() -> Self {
        // Empty counts as unset, same as S3_STORAGE_ACCESS_KEY: the
        // Dockerfile declares the variable empty so Bunny shows it.
        let api_key = std::env::var("BUNNY_API_KEY")
            .ok()
            .filter(|s| !s.trim().is_empty());

        if api_key.is_none() {
            tracing::warn!("BUNNY_API_KEY not set -- CDN cache purges are disabled");
        }

        Self { client: HttpClient::new(), api_key }
    }

    /// Purges one public URL from every edge cache of the pull zone.
    pub async fn purge(&self, public_url: &str) -> Result<(), AppError> {
        let Some(api_key) = &self.api_key else {
            tracing::warn!("Skipping CDN purge of {} (BUNNY_API_KEY not set)", public_url);
            return Ok(());
        };

        let url = reqwest::Url::parse_with_params("https://api.bunny.net/purge", &[("url", public_url)])
            .map_err(|e| {
                tracing::error!("Invalid purge URL {}: {}", public_url, e);
                AppError::internal("CDN purge failed")
            })?;

        let response = self.client
            .post(url)
            .header("AccessKey", api_key)
            .send()
            .await
            .map_err(|e| {
                tracing::error!("Bunny purge request error: {}", e);
                AppError::internal("CDN purge failed")
            })?;

        if !response.status().is_success() {
            tracing::error!("Bunny purge of {} failed: {}", public_url, response.status());
            return Err(AppError::internal("CDN purge failed"));
        }

        Ok(())
    }
}

// ─── NATIVE BUNNY REST API ───────────────────────────────────────────────────
// Hinweis: Seit Bunny eine S3-kompatible API anbietet, ist dieser REST-Wrapper
// redundant — S3Storage unten kann dasselbe Storage-Backend abdecken. Bleibt
// vorerst als Fallback erhalten.
#[derive(Clone)]
pub struct BunnyStorage {
    client: HttpClient,
    endpoint: String,
    bucket: String,
    api_key: String,
    public_url_base: String,
}

impl BunnyStorage {
    pub fn new() -> Self {
        let endpoint = std::env::var("BUNNY_STORAGE_ENDPOINT").expect("BUNNY_STORAGE_ENDPOINT missing");
        let bucket = std::env::var("BUNNY_STORAGE_BUCKET").expect("BUNNY_STORAGE_BUCKET missing");
        let api_key = std::env::var("BUNNY_STORAGE_ACCESS_KEY").expect("BUNNY_STORAGE_ACCESS_KEY missing");
        let public_url_base = std::env::var("BUNNY_PUBLIC_STORAGE_URL").expect("BUNNY_PUBLIC_STORAGE_URL missing");

        Self {
            client: HttpClient::new(),
            endpoint,
            bucket,
            api_key,
            public_url_base,
        }
    }

    pub async fn save(&self, key: &str, data: &[u8]) -> Result<(), AppError> {
        let url = format!("{}/{}/{}", self.endpoint, self.bucket, key);

        let content_type = if key.ends_with(".png") { "image/png" }
            else if key.ends_with(".webp") { "image/webp" }
            else { "image/jpeg" };

        let response = self.client
            .put(&url)
            .header("AccessKey", &self.api_key)
            .header("Content-Type", content_type)
            .body(data.to_vec())
            .send()
            .await
            .map_err(|e| {
                tracing::error!("Bunny API request error: {}", e);
                AppError::internal("Upload to CDN failed")
            })?;

        if !response.status().is_success() {
            let text = response.text().await.unwrap_or_default();
            tracing::error!("Bunny API rejected file: {}", text);
            return Err(AppError::internal("CDN rejected the file"));
        }

        Ok(())
    }

    pub async fn get(&self, key: &str) -> Result<Vec<u8>, AppError> {
        let url = format!("{}/{}/{}", self.endpoint, self.bucket, key);

        let response = self.client
            .get(&url)
            .header("AccessKey", &self.api_key)
            .send()
            .await
            .map_err(|e| {
                tracing::error!("Bunny API download error: {}", e);
                AppError::internal("Download from CDN failed")
            })?;

        if !response.status().is_success() {
            tracing::error!("Bunny API download failed for {}: {}", key, response.status());
            return Err(AppError::internal("CDN could not return file"));
        }

        let bytes = response.bytes().await.map_err(|e| {
            tracing::error!("Bunny API download body error: {}", e);
            AppError::internal("Download from CDN failed")
        })?;

        Ok(bytes.to_vec())
    }

    pub async fn delete(&self, key: &str) -> Result<(), AppError> {
        let url = format!("{}/{}/{}", self.endpoint, self.bucket, key);

        let response = self.client
            .delete(&url)
            .header("AccessKey", &self.api_key)
            .send()
            .await
            .map_err(|e| {
                tracing::error!("Bunny API delete error: {}", e);
                AppError::internal("Delete from CDN failed")
            })?;

        if !response.status().is_success() {
            tracing::error!("Bunny API delete failed: {}", response.status());
            return Err(AppError::internal("CDN could not delete file"));
        }

        Ok(())
    }

    pub fn public_url(&self, key: &str) -> String {
        let clean_key = key.strip_prefix('/').unwrap_or(key);
        format!("{}/{}", self.public_url_base, clean_key)
    }
}

// ─── S3 COMPATIBLE API (AWS SDK) ─────────────────────────────────────────────
// Konfiguriert für Bunny.net Storage über die S3-kompatible Gateway.
//
// Bunny-Mapping (wichtig!):
//   • Access Key ID     = Name deiner Storage Zone  → identisch mit dem Bucket.
//                         Darum reicht der Bucket; kein separater Access-Key nötig.
//                         (Setzt du S3_STORAGE_ACCESS_KEY, wird der genutzt — so
//                          funktioniert derselbe Code auch für Hetzner/AWS.)
//   • Secret Access Key = Passwort deiner Storage Zone (S3_STORAGE_SECRET_ACCESS_KEY)
//   • Endpoint          = https://<region>-s3.storage.bunnycdn.com
//   • Region            = de | ny | sg | uk | se | la | jh
//                         Wird aus dem Endpoint abgeleitet (override via S3_STORAGE_REGION).
//   • Bunny unterstützt NUR path-style URLs → force_path_style(true).
//
// Benötigte .env-Variablen:
//   STORAGE_PROVIDER=s3
//   S3_STORAGE_ENDPOINT=https://de-s3.storage.bunnycdn.com
//   S3_STORAGE_BUCKET=<deine-storage-zone>
//   S3_STORAGE_SECRET_ACCESS_KEY=<storage-zone-passwort>
//   S3_PUBLIC_STORAGE_URL=https://<deine-pullzone>.b-cdn.net   (öffentliche Reads laufen
//                         über die CDN-Pull-Zone, nicht über den S3-Endpoint!)
#[derive(Clone)]
pub struct S3Storage {
    client: S3Client,
    bucket: String,
    public_url_base: String,
}

impl S3Storage {
    pub async fn new() -> Self {
        let endpoint = std::env::var("S3_STORAGE_ENDPOINT").expect("S3_STORAGE_ENDPOINT missing");
        let bucket = std::env::var("S3_STORAGE_BUCKET").expect("S3_STORAGE_BUCKET missing");

        // Bunny: Access Key ID = Storage-Zone-Name = Bucket. Fallback auf den Bucket,
        // wenn kein expliziter Access-Key gesetzt ist. Leer zählt als nicht
        // gesetzt -- das Dockerfile deklariert die Variable leer, damit Bunny
        // sie anzeigt, und "" als Access-Key würde jede Signatur brechen.
        let access_key = std::env::var("S3_STORAGE_ACCESS_KEY")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| bucket.clone());

        // Secret = Storage-Zone-Passwort. Akzeptiert auch den alten Namen.
        let secret_key = std::env::var("S3_STORAGE_SECRET_ACCESS_KEY")
            .or_else(|_| std::env::var("S3_STORAGE_SECRET_KEY"))
            .expect("S3_STORAGE_SECRET_ACCESS_KEY missing");

        // Region wird für die Signatur gebraucht. Aus dem Endpoint ableiten
        // (z.B. https://de-s3.storage.bunnycdn.com → "de"), override via Env.
        let region_str = std::env::var("S3_STORAGE_REGION")
            .ok()
            .or_else(|| derive_region_from_endpoint(&endpoint))
            .unwrap_or_else(|| "us-east-1".to_string());

        let public_url_base = std::env::var("S3_PUBLIC_STORAGE_URL")
            .expect("S3_PUBLIC_STORAGE_URL missing");

        Self::connect(endpoint, bucket, access_key, secret_key, region_str, public_url_base).await
    }

    /// Builds the client from explicit settings, so the evidence zone
    /// (EvidenceStorage below) can reuse this with its own variables.
    async fn connect(
        endpoint: String,
        bucket: String,
        access_key: String,
        secret_key: String,
        region_str: String,
        public_url_base: String,
    ) -> Self {
        let credentials = Credentials::new(access_key, secret_key, None, None, "manual");

        let config = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .region(aws_config::Region::new(region_str))
            .credentials_provider(credentials)
            .endpoint_url(endpoint)
            .load()
            .await;

        let s3_config = aws_sdk_s3::config::Builder::from(&config)
            // WICHTIG: Bunny unterstützt nur path-style URLs.
            .force_path_style(true)
            .build();

        Self {
            client: S3Client::from_conf(s3_config),
            bucket,
            public_url_base,
        }
    }

    pub async fn save(&self, key: &str, data: &[u8]) -> Result<(), AppError> {
        let body = ByteStream::from(data.to_vec());
        let content_type = if key.ends_with(".png") { "image/png" }
            else if key.ends_with(".webp") { "image/webp" }
            else { "image/jpeg" };

        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .body(body)
            .content_type(content_type)
            .send()
            .await
            .map_err(|e| {
                tracing::error!("S3 upload error: {:?}", e);
                AppError::internal("Failed to upload via S3")
            })?;

        Ok(())
    }

    pub async fn get(&self, key: &str) -> Result<Vec<u8>, AppError> {
        let object = self.client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| {
                tracing::error!("S3 download error for {}: {:?}", key, e);
                AppError::internal("Failed to download via S3")
            })?;

        let body = object.body.collect().await.map_err(|e| {
            tracing::error!("S3 download body error for {}: {:?}", key, e);
            AppError::internal("Failed to download via S3")
        })?;

        Ok(body.into_bytes().to_vec())
    }

    pub async fn delete(&self, key: &str) -> Result<(), AppError> {
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| {
                tracing::error!("S3 delete error: {:?}", e);
                AppError::internal("Failed to delete via S3")
            })?;

        Ok(())
    }

    pub fn public_url(&self, key: &str) -> String {
        let clean_key = key.strip_prefix('/').unwrap_or(key);
        format!("{}/{}", self.public_url_base, clean_key)
    }
}

/// Leitet den Bunny-Regionscode aus dem S3-Endpoint ab.
/// z.B. "https://de-s3.storage.bunnycdn.com" → Some("de").
/// Gibt None zurück, wenn das Muster nicht passt (dann greift der Fallback).
fn derive_region_from_endpoint(endpoint: &str) -> Option<String> {
    let host = endpoint.split("://").last()?;      // de-s3.storage.bunnycdn.com
    let first_label = host.split('.').next()?;     // de-s3
    let region = first_label.strip_suffix("-s3")?; // de
    if region.is_empty() {
        None
    } else {
        Some(region.to_string())
    }
}

// ─── LOKALE FESTPLATTE (nur für lokale Entwicklung) ──────────────────────────
// Kein Netzwerkzugriff, kein Bunny-Key, keine Möglichkeit, versehentlich
// echte Prod-Dateien zu schreiben oder zu löschen — dafür gibt es diesen
// Provider. Dateien landen unter LOCAL_STORAGE_DIR (Default "./uploads",
// bereits in .gitignore), ausgeliefert über die /media-Route in routes.rs
// (nur gemountet, wenn STORAGE_PROVIDER=local).
//
// Für realistisch aussehende Daten lokal: scripts/sync_local_media.py lädt
// gezielt eine Stichprobe echter Dateien aus dem Prod-Bucket über einen
// Read-only-Key in genau dieses Verzeichnis herunter, mit denselben Keys,
// die auch schon in der lokal wiederhergestellten DB stehen — dieser
// Storage-Provider selbst braucht dafür keine Bunny-Credentials.
//
// Benötigte .env-Variablen:
//   STORAGE_PROVIDER=local
//   LOCAL_STORAGE_DIR=./uploads              (optional, das ist der Default)
//   LOCAL_STORAGE_PUBLIC_URL=http://localhost:3000
//     Absichtlich eine eigene Variable statt BASE_URL — BASE_URL zeigt in
//     der lokalen .env aktuell auf die echte Prod-Domain, das würde hier
//     kaputte Links erzeugen.
#[derive(Clone)]
pub struct LocalStorage {
    root: std::path::PathBuf,
    public_url_base: String,
}

impl LocalStorage {
    pub fn new() -> Self {
        let root = std::env::var("LOCAL_STORAGE_DIR").unwrap_or_else(|_| "./uploads".to_string());
        let public_url_base = std::env::var("LOCAL_STORAGE_PUBLIC_URL")
            .unwrap_or_else(|_| "http://localhost:3000".to_string());
        Self::at(&root, &public_url_base)
    }

    fn at(root: &str, public_url_base: &str) -> Self {
        let root = std::path::PathBuf::from(root);

        std::fs::create_dir_all(&root)
            .unwrap_or_else(|e| panic!("Konnte LOCAL_STORAGE_DIR '{}' nicht anlegen: {}", root.display(), e));

        Self {
            root,
            public_url_base: format!("{}/media", public_url_base.trim_end_matches('/')),
        }
    }

    pub async fn save(&self, key: &str, data: &[u8]) -> Result<(), AppError> {
        let path = self.safe_path(key)?;

        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                tracing::error!("Konnte Verzeichnis für {} nicht anlegen: {}", key, e);
                AppError::internal("Failed to save file")
            })?;
        }

        tokio::fs::write(&path, data).await.map_err(|e| {
            tracing::error!("Konnte {} nicht schreiben: {}", key, e);
            AppError::internal("Failed to save file")
        })?;

        Ok(())
    }

    pub async fn get(&self, key: &str) -> Result<Vec<u8>, AppError> {
        let path = self.safe_path(key)?;

        tokio::fs::read(&path).await.map_err(|e| {
            tracing::error!("Konnte {} nicht lesen: {}", key, e);
            AppError::internal("Failed to read file")
        })
    }

    pub async fn delete(&self, key: &str) -> Result<(), AppError> {
        let path = self.safe_path(key)?;

        match tokio::fs::remove_file(&path).await {
            Ok(()) => Ok(()),
            // Datei existiert schon nicht mehr — für den Aufrufer kein
            // Unterschied zu "erfolgreich gelöscht".
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => {
                tracing::error!("Konnte {} nicht löschen: {}", key, e);
                Err(AppError::internal("Failed to delete file"))
            }
        }
    }

    pub fn public_url(&self, key: &str) -> String {
        let clean_key = key.trim_start_matches('/');
        format!("{}/{}", self.public_url_base, clean_key)
    }

    /// Verhindert Path Traversal: ein Key wie "../../etc/passwd" würde
    /// sonst außerhalb von `root` landen. Prüft die Komponenten des Keys
    /// selbst (statt canonicalize, das eine bereits existierende Datei
    /// voraussetzt), lehnt jedes ".." darin ab, bevor überhaupt auf die
    /// Festplatte zugegriffen wird.
    fn safe_path(&self, key: &str) -> Result<std::path::PathBuf, AppError> {
        let trimmed = key.trim_start_matches('/');

        for component in std::path::Path::new(trimmed).components() {
            if matches!(component, std::path::Component::ParentDir) {
                return Err(AppError::bad_request("Invalid file key"));
            }
        }

        Ok(self.root.join(trimmed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signs_like_bunny() {
        // Reference computed independently in Python from Bunny's documented
        // scheme: base64url(sha256(key + path + expires)), unpadded.
        let signer = UrlSigner { key: "test-key".into() };
        assert_eq!(
            signer.sign("https://cdn.example/thumb/abc.webp", 1_790_000_000),
            "https://cdn.example/thumb/abc.webp?token=L2gbmmUhmb2F0tkhX7kJ0uHX-CDwu5MiAq5dMt-6tms&expires=1790035200"
        );
    }

    #[test]
    fn url_is_stable_within_a_bucket() {
        let signer = UrlSigner { key: "k".into() };
        let url = "https://cdn.example/full/x.webp";
        let start = 1_790_035_200; // a bucket boundary
        assert_eq!(signer.sign(url, start), signer.sign(url, start + URL_BUCKET_SECS - 1));
        assert_ne!(signer.sign(url, start), signer.sign(url, start + URL_BUCKET_SECS));
        // Valid for one to two buckets after issue.
        assert!(signer.sign(url, start).ends_with(&format!("expires={}", start + 2 * URL_BUCKET_SECS)));
    }
}
