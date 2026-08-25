/// Application configuration, loaded from environment variables.

pub struct Config {
    pub database_url: String, // Postgres connection string, e.g. postgres://user:pass@host:5432/db
    pub host: String,         // interface the HTTP server binds to, e.g. 0.0.0.0 to listen on all interfaces
    pub port: u16,            // TCP port the HTTP server listens on
    pub jwt_secret: String,   // HMAC signing secret used to sign and verify access tokens (see auth.rs)
    pub base_url: String,     // public base URL of this backend, used for building absolute links (e.g. in emails)
    // SMTP
    pub smtp_host: String,          // SMTP server hostname used to send transactional email
    pub smtp_port: u16,             // SMTP server port
    pub smtp_pass: Option<String>,  // SMTP password; None if unset or blank (see the smtp_pass line below)
    pub smtp_from: String,          // "From" address used on outgoing email
    // Redis: used for cross-replica pub/sub (real-time notifications).
    // Without this, SSE broadcast only reaches subscribers on the same
    // replica that handled the triggering request.
    pub redis_url: String,
}

impl Config {
    pub fn from_env() -> Self {
        // HOST and PORT have sane local defaults, so they use unwrap_or_else instead of
        // requiring the variable to be set.
        let host = std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
        let port: u16 = std::env::var("PORT")
            .unwrap_or_else(|_| "3000".to_string())
            .parse() // parse the string into a u16; panics via expect below if it is not a valid number
            .expect("PORT must be a number");

        Self {
            // DATABASE_URL has no sensible default, so a missing value is a hard error at startup.
            database_url: std::env::var("DATABASE_URL")
                .expect("DATABASE_URL must be set in .env"),
            // BASE_URL falls back to a localhost URL built from the port above, which is
            // convenient for local development but should always be set explicitly in production.
            base_url: std::env::var("BASE_URL")
                .unwrap_or_else(|_| format!("http://localhost:{}", port)),
            jwt_secret: {
                let secret = std::env::var("JWT_SECRET")
                    .expect("JWT_SECRET must be set");
                // Vars are declared empty in the image so Bunny surfaces them,
                // and std::env::var returns Ok("") for an unset-but-declared
                // value, which .expect() does NOT catch (an empty string is
                // still Ok, not Err). An empty signing secret would silently
                // produce forgeable tokens, so this explicit check catches
                // that case, on top of the .expect() catching the
                // "variable not set at all" case.
                assert!(!secret.trim().is_empty(), "JWT_SECRET must not be empty");
                secret
            },
            host,
            port,
            // SMTP defaults to MailHog (a local dev mail-catcher with no auth needed)
            smtp_host: std::env::var("SMTP_HOST")
                .unwrap_or_else(|_| "localhost".to_string()),
            smtp_port: std::env::var("SMTP_PORT")
                .unwrap_or_else(|_| "1025".to_string()) // MailHog's default SMTP port
                .parse()
                .expect("SMTP_PORT must be a number"),
            // Same empty-but-declared-var situation as JWT_SECRET above, but here an empty
            // password is valid (e.g. local MailHog needs no auth), so instead of asserting,
            // it is simply normalized to None via filter, so downstream code can treat
            // "not set" and "set to empty string" the same way.
            smtp_pass: std::env::var("SMTP_PASS").ok().filter(|s| !s.is_empty()),
            smtp_from: std::env::var("SMTP_FROM")
                .unwrap_or_else(|_| "noreply@klar.social".to_string()),
            // Defaults to a local Redis instance for development; production sets this
            // to the actual Upstash Redis URL.
            redis_url: std::env::var("REDIS_URL")
                .unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string()),
        }
    }

    // Builds the "host:port" string used to bind the HTTP server's listener.
    pub fn addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}
