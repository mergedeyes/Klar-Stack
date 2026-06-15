/// Email service — sends verification and password reset emails via SMTP.
///
/// Dev: MailHog on localhost:1025, view emails at http://localhost:8025
/// Prod: swap SMTP_HOST/SMTP_PORT/SMTP_USER/SMTP_PASS in .env

use lettre::{
    message::header::ContentType,
    transport::smtp::authentication::Credentials,
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
};

#[derive(Clone)]
pub struct EmailService {
    mailer: AsyncSmtpTransport<Tokio1Executor>,
    from_address: String,
    base_url: String,
}

#[derive(Debug)]
pub struct EmailError(pub String);

impl std::fmt::Display for EmailError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Email error: {}", self.0)
    }
}

impl EmailService {
    pub fn new(
        smtp_host: &str,
        smtp_port: u16,
        smtp_user: Option<&str>,
        smtp_pass: Option<&str>,
        from_address: &str,
        base_url: &str,
    ) -> Self {
        let mut transport = AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(smtp_host)
            .port(smtp_port);

        // Add credentials if provided (MailHog doesn't need them)
        if let (Some(user), Some(pass)) = (smtp_user, smtp_pass) {
            transport = transport.credentials(Credentials::new(
                user.to_string(),
                pass.to_string(),
            ));
        }

        let mailer = transport.build();

        Self {
            mailer,
            from_address: from_address.to_string(),
            base_url: base_url.to_string(),
        }
    }

    /// Send email verification link
    pub async fn send_verification(&self, to_email: &str, token: &str) -> Result<(), EmailError> {
        let verify_url = format!("{}/verify-email?token={}", self.base_url, token);

        let body = format!(
            "Willkommen bei Klar!\n\n\
             Bitte bestaetige deine E-Mail-Adresse:\n\n\
             {}\n\n\
             Der Link ist 24 Stunden gueltig.\n\n\
             Wenn du dich nicht bei Klar registriert hast, ignoriere diese E-Mail.",
            verify_url
        );

        self.send(to_email, "Bestaetige deine E-Mail bei Klar", &body).await
    }

    /// Send password reset link
    pub async fn send_password_reset(&self, to_email: &str, token: &str) -> Result<(), EmailError> {
        let reset_url = format!("{}/reset-password?token={}", self.base_url, token);

        let body = format!(
            "Passwort zuruecksetzen\n\n\
             Klicke auf den folgenden Link, um dein Passwort zurueckzusetzen:\n\n\
             {}\n\n\
             Der Link ist 1 Stunde gueltig.\n\n\
             Wenn du diese Anfrage nicht gestellt hast, ignoriere diese E-Mail.",
            reset_url
        );

        self.send(to_email, "Passwort zuruecksetzen bei Klar", &body).await
    }

    /// Send a plain text email
    async fn send(&self, to: &str, subject: &str, body: &str) -> Result<(), EmailError> {
        let email = Message::builder()
            .from(self.from_address.parse().map_err(|e| EmailError(format!("Invalid from: {}", e)))?)
            .to(to.parse().map_err(|e| EmailError(format!("Invalid to: {}", e)))?)
            .subject(subject)
            .header(ContentType::TEXT_PLAIN)
            .body(body.to_string())
            .map_err(|e| EmailError(format!("Failed to build email: {}", e)))?;

        self.mailer
            .send(email)
            .await
            .map_err(|e| EmailError(format!("Failed to send email: {}", e)))?;

        tracing::info!("Email sent to {}: {}", to, subject);
        Ok(())
    }
}
