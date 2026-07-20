/// Application configuration, loaded from environment variables.

pub struct Config {
    pub database_url: String,
    pub host: String,
    pub port: u16,
    pub jwt_secret: String,
    pub base_url: String,
    // SMTP
    pub smtp_host: String,
    pub smtp_port: u16,
    pub smtp_pass: Option<String>,
    pub smtp_from: String,
}

impl Config {
    pub fn from_env() -> Self {
        let host = std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
        let port: u16 = std::env::var("PORT")
            .unwrap_or_else(|_| "3000".to_string())
            .parse()
            .expect("PORT must be a number");

        Self {
            database_url: std::env::var("DATABASE_URL")
                .expect("DATABASE_URL must be set in .env"),
            base_url: std::env::var("BASE_URL")
                .unwrap_or_else(|_| format!("http://localhost:{}", port)),
            jwt_secret: std::env::var("JWT_SECRET")
                .expect("JWT_SECRET must be set in .env"),
            host,
            port,
            // SMTP defaults to MailHog
            smtp_host: std::env::var("SMTP_HOST")
                .unwrap_or_else(|_| "localhost".to_string()),
            smtp_port: std::env::var("SMTP_PORT")
                .unwrap_or_else(|_| "1025".to_string())
                .parse()
                .expect("SMTP_PORT must be a number"),
            smtp_pass: std::env::var("SMTP_PASS").ok(),
            smtp_from: std::env::var("SMTP_FROM")
                .unwrap_or_else(|_| "noreply@klar.social".to_string()),
        }
    }

    pub fn addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}
