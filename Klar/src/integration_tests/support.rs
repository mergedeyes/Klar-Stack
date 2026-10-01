//! The test harness: builds the app state on a test database, calls the
//! router in-process, and wraps the common steps (sign up, upload, report).

use std::future::Future;
use std::io::{Cursor, Read};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Once};
use std::time::Duration;

use axum::body::{to_bytes, Body, Bytes};
use axum::extract::ConnectInfo;
use axum::http::{header, HeaderMap, Method, Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

use crate::email::{EmailProvider, EmailService};
use crate::handlers::auth::AppState;
use crate::storage::{CdnPurger, EvidenceStorage, Storage};

pub const ADMIN_EMAIL: &str = "admin@example.test";
/// LEGAL_UPDATES_TOKEN for the deploy endpoint (handlers/legal_updates.rs).
pub const LEGAL_UPDATES_TOKEN: &str = "test-legal-updates-token";
pub const PASSWORD: &str = "test-password-123";

/// Every client gets its own address (X-Forwarded-For, one trusted hop),
/// so the per-IP rate limits never trip across a test's many sign-ups.
fn next_ip() -> String {
    static NEXT: AtomicU32 = AtomicU32::new(1);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    format!("10.{}.{}.{}", (n >> 16) & 255, (n >> 8) & 255, n & 255)
}

fn init_env() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        // Read on every admin check (utils::is_admin_email); the same value
        // for every test, so setting it once is race-free.
        std::env::set_var("ADMIN_EMAILS", ADMIN_EMAIL);
        std::env::set_var("LEGAL_UPDATES_TOKEN", LEGAL_UPDATES_TOKEN);
    });
}

/// Media and evidence folders, shared by apps built on the same test.
struct Dirs {
    media: tempfile::TempDir,
    evidence: tempfile::TempDir,
}

pub struct TestApp {
    pub state: AppState,
    router: Router,
    dirs: Arc<Dirs>,
}

pub struct Resp {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Bytes,
}

impl Resp {
    pub fn json(&self) -> Value {
        serde_json::from_slice(&self.body)
            .unwrap_or_else(|_| panic!("not JSON ({}): {}", self.status, String::from_utf8_lossy(&self.body)))
    }

    #[track_caller]
    pub fn ok(self) -> Self {
        assert!(self.status.is_success(), "expected success, got {}: {}", self.status, String::from_utf8_lossy(&self.body));
        self
    }
}

#[derive(Clone)]
pub struct User {
    pub id: Uuid,
    pub token: String,
    ip: String,
}

impl User {
    /// The same account with another access token, e.g. a second device's.
    pub fn with_token(&self, token: &str) -> User {
        User { id: self.id, token: token.to_string(), ip: self.ip.clone() }
    }
}

impl TestApp {
    pub async fn new(pool: PgPool) -> Self {
        let dirs = Arc::new(Dirs {
            media: tempfile::tempdir().unwrap(),
            evidence: tempfile::tempdir().unwrap(),
        });
        Self::build(pool, dirs, true).await
    }

    /// Another app on the same database and folders, with the evidence
    /// zone switched on or off (to test copies that wait for it).
    pub async fn with_evidence(&self, enabled: bool) -> Self {
        Self::build(self.state.db.clone(), self.dirs.clone(), enabled).await
    }

    async fn build(pool: PgPool, dirs: Arc<Dirs>, evidence: bool) -> Self {
        init_env();
        let redis_url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".into());
        let redis = redis::Client::open(redis_url)
            .unwrap()
            .get_connection_manager()
            .await
            .expect("integration tests need Redis at REDIS_URL");
        let (notification_tx, _) = tokio::sync::broadcast::channel(100);

        let state = AppState {
            db: pool,
            jwt_secret: "integration-test-jwt-secret".into(),
            storage: Storage::local_for_tests(dirs.media.path().to_str().unwrap()),
            cdn: CdnPurger::new(),
            evidence: EvidenceStorage::local_for_tests(evidence.then(|| dirs.evidence.path().to_str().unwrap())),
            // Nothing listens on port 9: sends fail fast and are only logged.
            email: EmailService::new(EmailProvider::Local, "127.0.0.1", 9, "test@klar.test", None, "http://klar.test"),
            notification_tx,
            redis,
        };
        let router = crate::routes::create_router(state.clone());
        Self { state, router, dirs }
    }

    pub fn media_file(&self, key: &str) -> PathBuf {
        self.dirs.media.path().join(key)
    }

    pub fn evidence_file(&self, key: &str) -> PathBuf {
        self.dirs.evidence.path().join(key)
    }

    // ── Requests ──────────────────────────────────────────────────────────────

    pub async fn send(&self, mut req: Request<Body>, ip: &str) -> Resp {
        req.headers_mut().insert("x-forwarded-for", ip.parse().unwrap());
        req.extensions_mut().insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 9999))));
        let res = self.router.clone().oneshot(req).await.unwrap();
        let status = res.status();
        let headers = res.headers().clone();
        let body = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        Resp { status, headers, body }
    }

    async fn json_request(&self, method: Method, path: &str, user: Option<&User>, body: Option<Value>) -> Resp {
        let mut req = Request::builder().method(method).uri(path);
        if let Some(user) = user {
            req = req.header(header::AUTHORIZATION, format!("Bearer {}", user.token));
        }
        let req = match body {
            Some(body) => req.header(header::CONTENT_TYPE, "application/json").body(Body::from(body.to_string())),
            None => req.body(Body::empty()),
        }
        .unwrap();
        let ip = user.map(|u| u.ip.clone()).unwrap_or_else(next_ip);
        self.send(req, &ip).await
    }

    pub async fn get(&self, user: &User, path: &str) -> Resp {
        self.json_request(Method::GET, path, Some(user), None).await
    }
    pub async fn post(&self, user: &User, path: &str, body: Value) -> Resp {
        self.json_request(Method::POST, path, Some(user), Some(body)).await
    }
    pub async fn patch(&self, user: &User, path: &str, body: Value) -> Resp {
        self.json_request(Method::PATCH, path, Some(user), Some(body)).await
    }
    pub async fn delete(&self, user: &User, path: &str) -> Resp {
        self.json_request(Method::DELETE, path, Some(user), None).await
    }
    pub async fn delete_with(&self, user: &User, path: &str, body: Value) -> Resp {
        self.json_request(Method::DELETE, path, Some(user), Some(body)).await
    }
    /// DELETE /users/me with the password it asks for.
    pub async fn delete_account(&self, user: &User) -> Resp {
        self.delete_with(user, "/users/me", json!({ "password": PASSWORD })).await
    }
    pub async fn anon_get(&self, path: &str) -> Resp {
        self.json_request(Method::GET, path, None, None).await
    }
    pub async fn anon_post(&self, path: &str, body: Value) -> Resp {
        self.json_request(Method::POST, path, None, Some(body)).await
    }

    /// The status of an anonymous GET without reading its body, for the
    /// notification stream, which never ends on its own.
    pub async fn anon_get_status(&self, path: &str) -> StatusCode {
        let mut req = Request::builder().method(Method::GET).uri(path).body(Body::empty()).unwrap();
        req.headers_mut().insert("x-forwarded-for", next_ip().parse().unwrap());
        req.extensions_mut().insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 9999))));
        self.router.clone().oneshot(req).await.unwrap().status()
    }

    // ── Common steps ──────────────────────────────────────────────────────────

    /// A new account with its email address verified, as almost every
    /// test needs: publishing, messaging and reporting require it.
    pub async fn register(&self, username: &str) -> User {
        let user = self.register_unverified(username).await;
        self.exec(&format!("UPDATE users SET email_verified = TRUE WHERE id = '{}'", user.id)).await;
        user
    }

    /// A new account that hasn't verified its address yet.
    pub async fn register_unverified(&self, username: &str) -> User {
        self.register_with_email(username, &format!("{username}@example.test")).await
    }

    pub async fn register_with_email(&self, username: &str, email: &str) -> User {
        let res = self
            .anon_post(
                "/auth/register",
                json!({ "username": username, "email": email, "password": PASSWORD, "accept_terms": true }),
            )
            .await
            .ok()
            .json();
        User {
            id: res["user"]["id"].as_str().unwrap().parse().unwrap(),
            token: res["access_token"].as_str().unwrap().into(),
            ip: next_ip(),
        }
    }

    /// An account whose reports restrict content before review (a hide or
    /// a warning, handlers/reports.rs): verified, like every `register`ed
    /// one, and older than a day. A fresh account's reports only queue.
    pub async fn trusted(&self, username: &str) -> User {
        let user = self.register(username).await;
        self.exec(&format!("UPDATE users SET created_at = NOW() - INTERVAL '2 days' WHERE id = '{}'", user.id)).await;
        user
    }

    /// An admin: the ADMIN_EMAILS address, with its email verified.
    pub async fn admin(&self) -> User {
        let admin = self.register_with_email("site_admin", ADMIN_EMAIL).await;
        self.exec(&format!("UPDATE users SET email_verified = TRUE WHERE id = '{}'", admin.id)).await;
        admin
    }

    pub async fn upload(&self, user: &User, caption: &str) -> Uuid {
        self.upload_image(user, caption, png(200, 150, [200, 60, 40])).await
    }

    pub async fn upload_image(&self, user: &User, caption: &str, image: Vec<u8>) -> Uuid {
        let res = self.try_upload(user, caption, image).await.ok().json();
        res["post"]["id"].as_str().unwrap().parse().unwrap()
    }

    /// POST /posts/upload, for checking a refusal.
    pub async fn try_upload(&self, user: &User, caption: &str, image: Vec<u8>) -> Resp {
        let req = multipart(
            "/posts/upload",
            user,
            &[("caption", None, caption.as_bytes().to_vec()), ("image", Some("photo.png"), image)],
        );
        self.send(req, &user.ip).await
    }

    pub async fn upload_avatar(&self, user: &User, rgb: [u8; 3]) {
        let req = multipart("/users/me/avatar", user, &[("avatar", Some("a.png"), png(200, 200, rgb))]);
        self.send(req, &user.ip).await.ok();
    }

    /// POST /feedback as the form sends it: text fields plus screenshots.
    pub async fn feedback(&self, user: &User, fields: &[(&str, &str)], screenshots: Vec<Vec<u8>>) -> Resp {
        let mut parts: Vec<(&str, Option<&str>, Vec<u8>)> =
            fields.iter().map(|(name, value)| (*name, None, value.as_bytes().to_vec())).collect();
        parts.extend(screenshots.into_iter().map(|png| ("screenshot", Some("shot.png"), png)));
        self.send(multipart("/feedback", user, &parts), &user.ip).await
    }

    pub async fn report(&self, user: &User, target_type: &str, target_id: Uuid, reason: &str) -> Uuid {
        let res = self
            .post(user, "/reports", json!({ "target_type": target_type, "target_id": target_id, "reason": reason }))
            .await
            .ok()
            .json();
        res["id"].as_str().unwrap().parse().unwrap()
    }

    pub async fn comment(&self, user: &User, post_id: Uuid, body: &str) -> Uuid {
        let res = self.post(user, &format!("/posts/{post_id}/comments"), json!({ "body": body })).await.ok().json();
        res["id"].as_str().unwrap().parse().unwrap()
    }

    /// The account's data export, parsed from the ZIP's data.json.
    pub async fn export(&self, user: &User) -> Value {
        let res = self.get(user, "/users/me/export").await.ok();
        let mut zip = zip::ZipArchive::new(Cursor::new(res.body.to_vec())).unwrap();
        let mut json = String::new();
        zip.by_name("data.json").unwrap().read_to_string(&mut json).unwrap();
        serde_json::from_str(&json).unwrap()
    }

    // ── Database ──────────────────────────────────────────────────────────────

    pub async fn exec(&self, sql: &str) {
        sqlx::query(sql).execute(&self.state.db).await.unwrap();
    }

    /// A single value as text (NULL as None), for asserting on state the
    /// API doesn't expose.
    pub async fn scalar(&self, sql: &str) -> Option<String> {
        sqlx::query_scalar::<_, Option<String>>(&format!("SELECT ({sql})::text"))
            .fetch_one(&self.state.db)
            .await
            .unwrap()
    }

    pub async fn count(&self, sql: &str) -> i64 {
        sqlx::query_scalar::<_, i64>(&format!("SELECT COUNT(*) FROM ({sql}) AS q"))
            .fetch_one(&self.state.db)
            .await
            .unwrap()
    }
}

/// Waits (up to 5 s) for background work, e.g. the evidence copy that
/// runs after a report's response.
pub async fn eventually<F, Fut>(what: &str, check: F)
where
    F: Fn() -> Fut,
    Fut: Future<Output = bool>,
{
    for _ in 0..100 {
        if check().await {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("timed out waiting for: {what}");
}

pub fn png(width: u32, height: u32, rgb: [u8; 3]) -> Vec<u8> {
    let img = image::RgbImage::from_pixel(width, height, image::Rgb(rgb));
    let mut buf = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(img).write_to(&mut buf, image::ImageFormat::Png).unwrap();
    buf.into_inner()
}

fn multipart(path: &str, user: &User, fields: &[(&str, Option<&str>, Vec<u8>)]) -> Request<Body> {
    const BOUNDARY: &str = "klar-test-boundary";
    let mut body = Vec::new();
    for (name, filename, data) in fields {
        body.extend_from_slice(format!("--{BOUNDARY}\r\n").as_bytes());
        match filename {
            Some(f) => body.extend_from_slice(
                format!("Content-Disposition: form-data; name=\"{name}\"; filename=\"{f}\"\r\nContent-Type: image/png\r\n\r\n").as_bytes(),
            ),
            None => body.extend_from_slice(format!("Content-Disposition: form-data; name=\"{name}\"\r\n\r\n").as_bytes()),
        }
        body.extend_from_slice(data);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{BOUNDARY}--\r\n").as_bytes());
    Request::builder()
        .method(Method::POST)
        .uri(path)
        .header(header::AUTHORIZATION, format!("Bearer {}", user.token))
        .header(header::CONTENT_TYPE, format!("multipart/form-data; boundary={BOUNDARY}"))
        .body(Body::from(body))
        .unwrap()
}
