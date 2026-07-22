/// Email service — sends verification and password reset emails.
///
/// Dev: MailHog on localhost:1025, view emails at http://localhost:8025
/// Prod: Resend (HTTPS API) — Bunny Magic Containers blocks outbound SMTP
/// ports (25/465/587/2525) by default, so raw SMTP relays (like IONOS) time
/// out from inside the container. Resend's API runs over plain HTTPS (443),
/// which is already open for everything else (image pulls, DB, etc).

use lettre::{
    message::header::ContentType,
    transport::smtp::authentication::Credentials,
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
};
use resend_rs::{types::CreateEmailBaseOptions, Resend};

#[derive(Clone)]
pub struct EmailService {
    transport: Transport,
    from_address: String,
    base_url: String,
}

#[derive(Clone)]
enum Transport {
    Smtp(AsyncSmtpTransport<Tokio1Executor>),
    Resend(Resend),
}

#[derive(Debug)]
pub struct EmailError(pub String);

impl std::fmt::Display for EmailError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Email error: {}", self.0)
    }
}

#[derive(Debug)]
pub enum EmailProvider {
    Local,
    Ionos,
    Resend,
}

impl std::str::FromStr for EmailProvider {
    type Err = String;
        fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "local" => Ok(Self::Local),
            "ionos" => Ok(Self::Ionos),
            "resend" => Ok(Self::Resend),
            _ => Err(format!("Unknown email provider: {}", s)),
        }
    }
}

impl EmailService {
    pub fn new(
        provider: EmailProvider,
        smtp_host: &str,
        smtp_port: u16,
        smtp_from: &str,
        // Reused as the Resend API key when provider == Resend (set via
        // SMTP_PASS env var either way — see config.rs)
        smtp_pass: Option<&str>,
        base_url: &str,
    ) -> Self {
        let transport = match provider {
            EmailProvider::Local => {
                Transport::Smtp(
                    AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(smtp_host)
                        .port(smtp_port)
                        .build()
                )
            }

            EmailProvider::Ionos => {
                let pass = smtp_pass.expect("SMTP_PASS required");

                Transport::Smtp(
                    AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(smtp_host)
                        .expect("Invalid SMTP host")
                        .port(smtp_port)
                        .credentials(Credentials::new(
                            smtp_from.to_string(),
                            pass.to_string(),
                        ))
                        .build()
                )
            }

            EmailProvider::Resend => {
                let api_key = smtp_pass.expect("SMTP_PASS (Resend API key) required");
                Transport::Resend(Resend::new(api_key))
            }
        };

        Self {
            transport,
            from_address: smtp_from.to_string(),
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
        match &self.transport {
            Transport::Smtp(mailer) => {
                let email = Message::builder()
                    .from(self.from_address.parse().map_err(|e| EmailError(format!("Invalid from: {}", e)))?)
                    .to(to.parse().map_err(|e| EmailError(format!("Invalid to: {}", e)))?)
                    .subject(subject)
                    .header(ContentType::TEXT_PLAIN)
                    .body(body.to_string())
                    .map_err(|e| EmailError(format!("Failed to build email: {}", e)))?;

                mailer
                    .send(email)
                    .await
                    .map_err(|e| EmailError(format!("Failed to send email: {}", e)))?;
            }

            Transport::Resend(resend) => {
                let email = CreateEmailBaseOptions::new(
                    self.from_address.as_str(),
                    [to],
                    subject,
                )
                .with_text(body);

                resend
                    .emails
                    .send(email)
                    .await
                    .map_err(|e| EmailError(format!("Resend API error: {}", e)))?;
            }
        }

        tracing::info!("Email sent to {}: {}", to, subject);
        Ok(())
    }
}
