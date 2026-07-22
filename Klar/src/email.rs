/// Email service — sends verification and password reset emails.
///
/// Dev: MailHog on localhost:1025, view emails at http://localhost:8025
/// Prod: Resend or Scaleway TEM (HTTPS APIs) — Bunny Magic Containers blocks
/// outbound SMTP ports (25/465/587/2525) by default, so raw SMTP relays
/// (like IONOS) time out from inside the container. Both providers' APIs
/// run over plain HTTPS (443), which is already open for everything else
/// (image pulls, DB, etc). Scaleway TEM is the EU-data-residency option —
/// unlike Resend, both sending *and* account/log data stay in the EU.

use lettre::{
    message::{header::ContentType, MultiPart, SinglePart},
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
    ScalewayTem {
        client: reqwest::Client,
        secret_key: String,
        project_id: String,
        region: String,
    },
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
    ScalewayTem,
}

impl std::str::FromStr for EmailProvider {
    type Err = String;
        fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "local" => Ok(Self::Local),
            "ionos" => Ok(Self::Ionos),
            "resend" => Ok(Self::Resend),
            "scaleway" | "tem" | "scaleway_tem" => Ok(Self::ScalewayTem),
            _ => Err(format!("Unknown email provider: {}", s)),
        }
    }
}

/// Wraps email content in the shared HTML template (adapted from the
/// well-known htmlemail.io transactional template). Keeps a single place
/// for the CSS/layout so verification and password-reset emails stay
/// visually consistent.
fn render_html_email(preheader: &str, intro: &str, button_label: &str, button_url: &str, note: &str) -> String {
    format!(
        r#"<!doctype html>
<html lang="de">
  <head>
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <meta http-equiv="Content-Type" content="text/html; charset=UTF-8">
    <title>Klar</title>
    <style media="all" type="text/css">
    body {{
      font-family: Helvetica, sans-serif;
      -webkit-font-smoothing: antialiased;
      font-size: 16px;
      line-height: 1.3;
      -ms-text-size-adjust: 100%;
      -webkit-text-size-adjust: 100%;
      background-color: #f4f5f6;
      margin: 0;
      padding: 0;
    }}
    table {{
      border-collapse: separate;
      mso-table-lspace: 0pt;
      mso-table-rspace: 0pt;
      width: 100%;
    }}
    table td {{
      font-family: Helvetica, sans-serif;
      font-size: 16px;
      vertical-align: top;
    }}
    .body {{
      background-color: #f4f5f6;
      width: 100%;
    }}
    .container {{
      margin: 0 auto !important;
      max-width: 600px;
      padding: 0;
      padding-top: 24px;
      width: 600px;
    }}
    .content {{
      box-sizing: border-box;
      display: block;
      margin: 0 auto;
      max-width: 600px;
      padding: 0;
    }}
    .main {{
      background: #ffffff;
      border: 1px solid #eaebed;
      border-radius: 16px;
      width: 100%;
    }}
    .wrapper {{
      box-sizing: border-box;
      padding: 24px;
    }}
    .footer {{
      clear: both;
      padding-top: 24px;
      text-align: center;
      width: 100%;
    }}
    .footer td, .footer p, .footer span, .footer a {{
      color: #9a9ea6;
      font-size: 14px;
      text-align: center;
    }}
    p {{
      font-family: Helvetica, sans-serif;
      font-size: 16px;
      font-weight: normal;
      margin: 0;
      margin-bottom: 16px;
    }}
    a {{
      color: #0867ec;
      text-decoration: underline;
    }}
    .btn {{
      box-sizing: border-box;
      min-width: 100% !important;
      width: 100%;
    }}
    .btn > tbody > tr > td {{
      padding-bottom: 16px;
    }}
    .btn table {{
      width: auto;
    }}
    .btn table td {{
      background-color: #ffffff;
      border-radius: 4px;
      text-align: center;
    }}
    .btn a {{
      background-color: #ffffff;
      border: solid 2px #0867ec;
      border-radius: 4px;
      box-sizing: border-box;
      color: #0867ec;
      cursor: pointer;
      display: inline-block;
      font-size: 16px;
      font-weight: bold;
      margin: 0;
      padding: 12px 24px;
      text-decoration: none;
      text-transform: capitalize;
    }}
    .btn-primary table td {{
      background-color: #0867ec;
    }}
    .btn-primary a {{
      background-color: #0867ec;
      border-color: #0867ec;
      color: #ffffff;
    }}
    .preheader {{
      color: transparent;
      display: none;
      height: 0;
      max-height: 0;
      max-width: 0;
      opacity: 0;
      overflow: hidden;
      mso-hide: all;
      visibility: hidden;
      width: 0;
    }}
    @media only screen and (max-width: 640px) {{
      .wrapper {{ padding: 8px !important; }}
      .content {{ padding: 0 !important; }}
      .container {{ padding: 0 !important; padding-top: 8px !important; width: 100% !important; }}
      .main {{ border-left-width: 0 !important; border-radius: 0 !important; border-right-width: 0 !important; }}
      .btn table, .btn a {{ max-width: 100% !important; width: 100% !important; }}
    }}
    </style>
  </head>
  <body>
    <table role="presentation" border="0" cellpadding="0" cellspacing="0" class="body">
      <tr>
        <td>&nbsp;</td>
        <td class="container">
          <div class="content">
            <span class="preheader">{preheader}</span>
            <table role="presentation" border="0" cellpadding="0" cellspacing="0" class="main">
              <tr>
                <td class="wrapper">
                  <p>{intro}</p>
                  <table role="presentation" border="0" cellpadding="0" cellspacing="0" class="btn btn-primary">
                    <tbody>
                      <tr>
                        <td align="left">
                          <table role="presentation" border="0" cellpadding="0" cellspacing="0">
                            <tbody>
                              <tr>
                                <td> <a href="{button_url}" target="_blank">{button_label}</a> </td>
                              </tr>
                            </tbody>
                          </table>
                        </td>
                      </tr>
                    </tbody>
                  </table>
                  <p>{note}</p>
                </td>
              </tr>
            </table>
            <div class="footer">
              <table role="presentation" border="0" cellpadding="0" cellspacing="0">
                <tr>
                  <td class="content-block">
                    Diese E-Mail wurde automatisch von Klar versendet.
                  </td>
                </tr>
              </table>
            </div>
          </div>
        </td>
        <td>&nbsp;</td>
      </tr>
    </table>
  </body>
</html>"#,
        preheader = preheader,
        intro = intro,
        button_url = button_url,
        button_label = button_label,
        note = note,
    )
}

impl EmailService {
    pub fn new(
        provider: EmailProvider,
        smtp_host: &str,
        smtp_port: u16,
        smtp_from: &str,
        // Reused across providers rather than adding new fn params:
        // - Ionos: SMTP password
        // - Resend: API key
        // - ScalewayTem: "SECRET_KEY|PROJECT_ID" (Scaleway needs both a
        //   secret key and a project ID to send — packed into this one
        //   slot since EmailService::new()'s signature is otherwise shared
        //   across every provider). smtp_host doubles as the region
        //   (e.g. "fr-par"), defaulting to "fr-par" if left empty.
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

            EmailProvider::ScalewayTem => {
                let packed = smtp_pass.expect("SMTP_PASS (\"SECRET_KEY|PROJECT_ID\") required for Scaleway TEM");
                let (secret_key, project_id) = packed
                    .split_once('|')
                    .expect("SMTP_PASS for Scaleway TEM must be \"SECRET_KEY|PROJECT_ID\"");

                let region = if smtp_host.is_empty() { "fr-par" } else { smtp_host };

                Transport::ScalewayTem {
                    client: reqwest::Client::new(),
                    secret_key: secret_key.to_string(),
                    project_id: project_id.to_string(),
                    region: region.to_string(),
                }
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

        let text = format!(
            "Willkommen bei Klar!\n\n\
             Bitte bestaetige deine E-Mail-Adresse:\n\n\
             {}\n\n\
             Der Link ist 24 Stunden gueltig.\n\n\
             Wenn du dich nicht bei Klar registriert hast, ignoriere diese E-Mail.",
            verify_url
        );

        let html = render_html_email(
            "Bestaetige deine E-Mail-Adresse bei Klar",
            "Willkommen bei Klar! Bitte bestaetige deine E-Mail-Adresse, um loszulegen.",
            "E-Mail bestaetigen",
            &verify_url,
            "Der Link ist 24 Stunden gueltig. Wenn du dich nicht bei Klar registriert hast, ignoriere diese E-Mail.",
        );

        self.send(to_email, "Bestaetige deine E-Mail bei Klar", &text, &html).await
    }

    /// Send password reset link
    pub async fn send_password_reset(&self, to_email: &str, token: &str) -> Result<(), EmailError> {
        let reset_url = format!("{}/reset-password?token={}", self.base_url, token);

        let text = format!(
            "Passwort zuruecksetzen\n\n\
             Klicke auf den folgenden Link, um dein Passwort zurueckzusetzen:\n\n\
             {}\n\n\
             Der Link ist 1 Stunde gueltig.\n\n\
             Wenn du diese Anfrage nicht gestellt hast, ignoriere diese E-Mail.",
            reset_url
        );

        let html = render_html_email(
            "Setze dein Passwort bei Klar zurueck",
            "Du hast angefragt, dein Passwort bei Klar zurueckzusetzen.",
            "Passwort zuruecksetzen",
            &reset_url,
            "Der Link ist 1 Stunde gueltig. Wenn du diese Anfrage nicht gestellt hast, ignoriere diese E-Mail.",
        );

        self.send(to_email, "Passwort zuruecksetzen bei Klar", &text, &html).await
    }

    /// Send an email with both plain-text and HTML alternatives
    async fn send(&self, to: &str, subject: &str, text: &str, html: &str) -> Result<(), EmailError> {
        match &self.transport {
            Transport::Smtp(mailer) => {
                let email = Message::builder()
                    .from(self.from_address.parse().map_err(|e| EmailError(format!("Invalid from: {}", e)))?)
                    .to(to.parse().map_err(|e| EmailError(format!("Invalid to: {}", e)))?)
                    .subject(subject)
                    .multipart(
                        MultiPart::alternative()
                            .singlepart(
                                SinglePart::builder()
                                    .header(ContentType::TEXT_PLAIN)
                                    .body(text.to_string()),
                            )
                            .singlepart(
                                SinglePart::builder()
                                    .header(ContentType::TEXT_HTML)
                                    .body(html.to_string()),
                            ),
                    )
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
                .with_html(html)
                .with_text(text);

                resend
                    .emails
                    .send(email)
                    .await
                    .map_err(|e| EmailError(format!("Resend API error: {}", e)))?;
            }

            Transport::ScalewayTem { client, secret_key, project_id, region } => {
                // Scaleway TEM has no official Rust SDK, so this calls its
                // plain REST API directly. Schema per their docs:
                // POST /transactional-email/v1alpha1/regions/{region}/emails
                // Auth via X-Auth-Token header (not Bearer).
                // Note: Scaleway requires subjects to be at least 10
                // characters — both of ours already clear that easily.
                let payload = serde_json::json!({
                    "from": { "email": self.from_address },
                    "to": [{ "email": to }],
                    "subject": subject,
                    "text": text,
                    "html": html,
                    "project_id": project_id,
                });

                let body_bytes = serde_json::to_vec(&payload)
                    .map_err(|e| EmailError(format!("Failed to serialize request: {}", e)))?;

                let url = format!(
                    "https://api.scaleway.com/transactional-email/v1alpha1/regions/{}/emails",
                    region
                );

                let response = client
                    .post(&url)
                    .header("X-Auth-Token", secret_key)
                    .header("Content-Type", "application/json")
                    .body(body_bytes)
                    .send()
                    .await
                    .map_err(|e| EmailError(format!("Scaleway TEM request failed: {}", e)))?;

                if !response.status().is_success() {
                    let status = response.status();
                    let text_body = response.text().await.unwrap_or_default();
                    return Err(EmailError(format!(
                        "Scaleway TEM API error ({}): {}",
                        status, text_body
                    )));
                }
            }
        }

        tracing::info!("Email sent to {}: {}", to, subject);
        Ok(())
    }
}
