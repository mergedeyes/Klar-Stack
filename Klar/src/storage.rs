use aws_sdk_s3::{config::Credentials, Client as S3Client};
use aws_sdk_s3::primitives::ByteStream;
use reqwest::Client as HttpClient;
use crate::errors::AppError;

// ─── DER WRAPPER ─────────────────────────────────────────────────────────────
// Diese Struktur wird im AppState gespeichert. Sie leitet jeden Aufruf
// einfach an den aktiven Provider weiter.
#[derive(Clone)]
pub enum Storage {
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

        match provider.as_str() {
            "s3" => {
                tracing::info!("Storage Backend: S3 Compatible");
                Storage::S3(S3Storage::new().await)
            }
            "local" => {
                tracing::info!("Storage Backend: Lokale Festplatte (nur Dev)");
                Storage::Local(LocalStorage::new())
            }
            _ => {
                tracing::info!("Storage Backend: Bunny.net REST API");
                Storage::Bunny(BunnyStorage::new())
            }
        }
    }

    pub async fn save(&self, key: &str, data: &[u8]) -> Result<(), AppError> {
        match self {
            Storage::S3(s) => s.save(key, data).await,
            Storage::Bunny(b) => b.save(key, data).await,
            Storage::Local(l) => l.save(key, data).await,
        }
    }

    pub async fn delete(&self, key: &str) -> Result<(), AppError> {
        match self {
            Storage::S3(s) => s.delete(key).await,
            Storage::Bunny(b) => b.delete(key).await,
            Storage::Local(l) => l.delete(key).await,
        }
    }

    pub fn public_url(&self, key: &str) -> String {
        match self {
            Storage::S3(s) => s.public_url(key),
            Storage::Bunny(b) => b.public_url(key),
            Storage::Local(l) => l.public_url(key),
        }
    }

    /// Resolves an optional storage key into a full public URL, passing
    /// None through unchanged. Most media/avatar fields are optional (not
    /// every post has media yet, not every user has an avatar), so this
    /// saves every call site from repeating the same Option handling
    /// around public_url(). Used by the ResolveMedia impls in utils.rs.
    pub fn resolve(&self, key: Option<String>) -> Option<String> {
        key.map(|k| self.public_url(&k))
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
        let clean_key = if key.starts_with('/') { &key[1..] } else { key };
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
        // wenn kein expliziter Access-Key gesetzt ist.
        let access_key = std::env::var("S3_STORAGE_ACCESS_KEY")
            .unwrap_or_else(|_| bucket.clone());

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
        let clean_key = if key.starts_with('/') { &key[1..] } else { key };
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
        let root = std::path::PathBuf::from(root);

        std::fs::create_dir_all(&root)
            .unwrap_or_else(|e| panic!("Konnte LOCAL_STORAGE_DIR '{}' nicht anlegen: {}", root.display(), e));

        let public_url_base = std::env::var("LOCAL_STORAGE_PUBLIC_URL")
            .unwrap_or_else(|_| "http://localhost:3000".to_string());

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
