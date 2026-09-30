//! Email service: sends verification and password reset emails.
//!
//! Dev: MailHog on localhost:1025, view emails at http://localhost:8025
//! Prod: Scaleway TEM (HTTPS APIs). Bunny Magic Containers blocks
//! outbound SMTP ports (25/465/587/2525) by default, so raw SMTP relays
//! (like IONOS) time out from inside the container. Both providers' APIs
//! run over plain HTTPS (443), which is already open for everything else
//! (image pulls, DB, etc). Scaleway TEM is the EU-data-residency option:
//! sending and account/log data stays in the EU.

use lettre::{
    message::{header::ContentType, MultiPart, SinglePart}, // building a multipart (text + HTML) email body
    transport::smtp::authentication::Credentials, // username/password pair for SMTP auth
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor, // async SMTP client, its send trait, the email type, and the tokio executor it runs on
};
use resend_rs::{types::CreateEmailBaseOptions, Resend}; // client for the Resend HTTPS email API

// The service is provider-agnostic from the caller's point of view: callers
// just call send_verification/send_password_reset, and this struct decides
// which underlying transport actually delivers the email.
#[derive(Clone)]
pub struct EmailService {
    transport: Transport,   // which provider/connection to send through (see Transport below)
    from_address: String,   // the "From" address used on every outgoing email
    base_url: String,       // public base URL, used to build verification/reset links
    // Where replies go: the From address (noreply@) has no inbox, so a user
    // answering a moderation notice or a lock email would otherwise be
    // talking to nobody. None: no Reply-To header.
    reply_to: Option<String>,
}

// One variant per supported way of actually delivering an email.
#[derive(Clone)]
enum Transport {
    Smtp(AsyncSmtpTransport<Tokio1Executor>), // used for both local MailHog and IONOS SMTP relay
    Resend(Resend),                            // Resend's own HTTPS API client
    ScalewayTem {
        // Scaleway has no Rust SDK, so this transport calls their REST API manually (see send() below)
        client: reqwest::Client,
        secret_key: String,
        project_id: String,
        region: String,
    },
}

// Simple string-wrapping error type; all provider-specific errors get
// normalized into this one type so callers only have to handle one error shape.
#[derive(Debug)]
pub struct EmailError(pub String);

impl std::fmt::Display for EmailError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Email error: {}", self.0)
    }
}

/// Which email provider to use, selected via config/env at startup.
#[derive(Debug)]
pub enum EmailProvider {
    Local,       // MailHog, for local development
    Ionos,       // SMTP relay, currently unusable in prod (see module doc comment above)
    Resend,
    ScalewayTem,
}

// Lets EmailProvider be parsed from a plain string (e.g. an env var value),
// case-insensitively, with a couple of accepted aliases for Scaleway TEM.
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
// The template itself is plain HTML/CSS below; the only Rust part is the
// five {placeholder} slots filled in via format! at the bottom of the function.
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
        //   secret key and a project ID to send, packed into this one
        //   slot since EmailService::new()'s signature is otherwise shared
        //   across every provider). smtp_host doubles as the region
        //   (e.g. "fr-par"), defaulting to "fr-par" if left empty.
        smtp_pass: Option<&str>,
        base_url: &str,
    ) -> Self {
        // Build the right Transport variant for the configured provider. Each arm
        // sets up whatever that provider actually needs to authenticate and send.
        let transport = match provider {
            EmailProvider::Local => {
                // builder_dangerous skips TLS certificate validation, fine for a local
                // MailHog instance but not something to use against a real mail server.
                Transport::Smtp(
                    AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(smtp_host)
                        .port(smtp_port)
                        .build()
                )
            }

            EmailProvider::Ionos => {
                let pass = smtp_pass.expect("SMTP_PASS required");

                // starttls_relay sets up a proper encrypted SMTP connection with
                // certificate validation, plus username/password auth.
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
                // Resend just needs an API key, no host/port involved.
                let api_key = smtp_pass.expect("SMTP_PASS (Resend API key) required");
                Transport::Resend(Resend::new(api_key))
            }

            EmailProvider::ScalewayTem => {
                // Unpack the "SECRET_KEY|PROJECT_ID" value described in the doc comment above.
                let packed = smtp_pass.expect("SMTP_PASS (\"SECRET_KEY|PROJECT_ID\") required for Scaleway TEM");
                let (secret_key, project_id) = packed
                    .split_once('|')
                    .expect("SMTP_PASS for Scaleway TEM must be \"SECRET_KEY|PROJECT_ID\"");

                // smtp_host is repurposed to carry the Scaleway region here, falling back
                // to Paris if not set.
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
            reply_to: None,
        }
    }

    /// Sets the Reply-To address for every email except the verification
    /// email (see `send_verification`). Empty: none.
    pub fn with_reply_to(mut self, address: &str) -> Self {
        let address = address.trim();
        self.reply_to = (!address.is_empty()).then(|| address.to_string());
        self
    }

    /// Send email verification link
    pub async fn send_verification(&self, to_email: &str, token: &str) -> Result<(), EmailError> {
        // Build the link the user clicks to verify their address.
        let verify_url = format!("{}/verify-email?token={}", self.base_url, token);

        // Plain-text fallback body, for mail clients that don't render HTML.
        let text = format!(
            "Willkommen bei Klar!\n\n\
             Bitte bestaetige deine E-Mail-Adresse:\n\n\
             {}\n\n\
             Der Link ist 24 Stunden gueltig.\n\n\
             Wenn du dich nicht bei Klar registriert hast, ignoriere diese E-Mail.",
            verify_url
        );

        // HTML body, built from the shared template above.
        let html = render_html_email(
            "Bestaetige deine E-Mail-Adresse bei Klar",
            "Willkommen bei Klar! Bitte bestaetige deine E-Mail-Adresse, um loszulegen.",
            "E-Mail bestaetigen",
            &verify_url,
            "Der Link ist 24 Stunden gueltig. Wenn du dich nicht bei Klar registriert hast, ignoriere diese E-Mail.",
        );

        // No Reply-To: replies to a verification email only ever mean
        // "here's my code" or bounce noise, and noreply@ has no inbox.
        self.send_with(to_email, "Bestaetige deine E-Mail bei Klar", &text, &html, None).await
    }

    /// Tell a user that a moderation decision affects their content (DSA
    /// Art. 17). The statement itself is only on the linked page, so the
    /// email reveals nothing beyond "there is a decision" if it's read by
    /// someone else.
    pub async fn send_moderation_notice(&self, to_email: &str, decision_id: uuid::Uuid) -> Result<(), EmailError> {
        let url = format!("{}/moderation/decisions/{}", self.base_url, decision_id);

        let text = format!(
            "Moderationsentscheidung zu deinem Inhalt\n\n\
             Wir haben eine Entscheidung zu einem deiner Inhalte bei Klar getroffen. \
             Die Begruendung und wie du widersprechen kannst, findest du hier:\n\n\
             {}\n\n\
             Du musst dafuer angemeldet sein.",
            url
        );

        let html = render_html_email(
            "Eine Moderationsentscheidung zu deinem Inhalt bei Klar",
            "Wir haben eine Entscheidung zu einem deiner Inhalte bei Klar getroffen. Die Begruendung und wie du widersprechen kannst, findest du unter dem folgenden Link.",
            "Begruendung ansehen",
            &url,
            "Du musst dafuer bei Klar angemeldet sein.",
        );

        self.send(to_email, "Moderationsentscheidung zu deinem Inhalt bei Klar", &text, &html).await
    }

    /// Two weeks before a permanently suspended account is deleted: when,
    /// and that the data can still be exported or the decision objected to.
    pub async fn send_ban_deletion_notice(
        &self,
        to_email: &str,
        decision_id: uuid::Uuid,
        deletion_on: &str,
    ) -> Result<(), EmailError> {
        let url = format!("{}/moderation/decisions/{}", self.base_url, decision_id);

        let text = format!(
            "Dein Klar-Konto wird am {} geloescht\n\n\
             Dein Konto ist dauerhaft gesperrt. Nach Ablauf der Widerspruchsfrist loeschen wir es am {} \
             mit allen Inhalten. Bis dahin kannst du in den Einstellungen deine Daten exportieren oder \
             der Entscheidung widersprechen:\n\n\
             {}\n\n\
             Du musst dafuer angemeldet sein.",
            deletion_on, deletion_on, url
        );

        let html = render_html_email(
            &format!("Dein Klar-Konto wird am {} gelöscht", deletion_on),
            &format!(
                "Dein Konto ist dauerhaft gesperrt. Nach Ablauf der Widerspruchsfrist löschen wir es am {} mit allen Inhalten. Bis dahin kannst du in den Einstellungen deine Daten exportieren oder der Entscheidung widersprechen.",
                deletion_on
            ),
            "Entscheidung ansehen",
            &url,
            "Du musst dafuer bei Klar angemeldet sein.",
        );

        self.send(to_email, "Dein Klar-Konto wird bald geloescht", &text, &html).await
    }

    /// Confirm receipt of a rights claim (DSA Art. 16(4)) with the private
    /// status link. The token is in the URL fragment, which browsers never
    /// send to a server, so it can't end up in any access log.
    pub async fn send_rights_claim_received(&self, to_email: &str, claim_id: uuid::Uuid, token: &str) -> Result<(), EmailError> {
        let url = format!("{}/rights/status/{}#{}", self.base_url, claim_id, token);

        let text = format!(
            "Deine Rechte-Meldung ist bei Klar eingegangen\n\n\
             Wir pruefen deine Meldung und informieren dich per E-Mail ueber das Ergebnis. \
             Den Stand kannst du jederzeit hier ansehen:\n\n\
             {}\n\n\
             Bewahre diesen Link auf und gib ihn nicht weiter -- er ist dein Zugang zu dieser Meldung.",
            url
        );
        let html = render_html_email(
            "Deine Rechte-Meldung ist bei Klar eingegangen",
            "Wir pruefen deine Meldung und informieren dich per E-Mail ueber das Ergebnis. Den Stand kannst du jederzeit ueber den folgenden Link ansehen.",
            "Stand ansehen",
            &url,
            "Bewahre diesen Link auf und gib ihn nicht weiter -- er ist dein Zugang zu dieser Meldung.",
        );
        self.send(to_email, "Deine Rechte-Meldung ist bei Klar eingegangen", &text, &html).await
    }

    /// A step in a rights claim (evidence request, decision, restoration),
    /// with the message itself in the email. The button leads to the status
    /// page, which needs the link from the confirmation email.
    pub async fn send_rights_claim_update(
        &self,
        to_email: &str,
        claim_id: uuid::Uuid,
        subject: &str,
        message: &str,
    ) -> Result<(), EmailError> {
        let url = format!("{}/rights/status/{}", self.base_url, claim_id);
        let text = format!(
            "{}\n\n{}\n\n\
             Antworten oder den Stand ansehen kannst du ueber den Link aus deiner Bestaetigungs-E-Mail.",
            subject, message
        );
        let escaped = message.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
        let html = render_html_email(
            subject,
            &escaped,
            "Zur Meldung",
            &url,
            "Antworten oder den Stand ansehen kannst du ueber den Link aus deiner Bestaetigungs-E-Mail.",
        );
        self.send(to_email, subject, &text, &html).await
    }

    /// Tell the owner of an account we locked because we suspect someone
    /// else is using it: why, what happened (all devices signed out), and
    /// the reset link that unlocks it. Replies go to the contact address,
    /// for owners who can't use the link (e.g. their inbox was taken over
    /// too).
    pub async fn send_account_locked(&self, to_email: &str, token: &str) -> Result<(), EmailError> {
        let reset_url = format!("{}/reset-password?token={}", self.base_url, token);
        let contact = self.reply_to.as_deref().unwrap_or("kontakt@klarsocial.eu");

        let text = format!(
            "Wir haben dein Klar-Konto vorsorglich gesperrt\n\n\
             Auf deinem Konto gab es Aktivitaet, die darauf hindeutet, dass jemand anderes es benutzt. \
             Wir haben dich deshalb auf allen Geraeten abgemeldet und das Konto gesperrt.\n\n\
             Mit einem neuen Passwort entsperrst du es wieder:\n\n\
             {}\n\n\
             Der Link ist 24 Stunden gueltig; danach kannst du beim Anmelden einen neuen anfordern. \
             Nimm ein Passwort, das du nirgends sonst verwendest.\n\n\
             Kommst du nicht weiter oder hast du Fragen, antworte auf diese E-Mail oder schreib an {}.",
            reset_url, contact
        );

        let html = render_html_email(
            "Wir haben dein Klar-Konto vorsorglich gesperrt",
            "Auf deinem Konto gab es Aktivität, die darauf hindeutet, dass jemand anderes es benutzt. Wir haben dich deshalb auf allen Geräten abgemeldet und das Konto gesperrt. Mit einem neuen Passwort entsperrst du es wieder.",
            "Neues Passwort festlegen",
            &reset_url,
            &format!(
                "Der Link ist 24 Stunden gültig; danach kannst du beim Anmelden einen neuen anfordern. Nimm ein Passwort, das du nirgends sonst verwendest. Kommst du nicht weiter, antworte auf diese E-Mail oder schreib an {}.",
                contact
            ),
        );

        self.send(to_email, "Dein Klar-Konto wurde vorsorglich gesperrt", &text, &html).await
    }

    /// Tell a user that the Terms of Service and/or the privacy policy
    /// changed: what changed (the admin's summary) and where to read it.
    /// For Terms changes, that they're asked to accept on their next visit.
    pub async fn send_legal_update(
        &self,
        to_email: &str,
        documents: &[String],
        summary: &str,
        requires_acceptance: bool,
    ) -> Result<(), EmailError> {
        let terms = documents.iter().any(|d| d == "terms");
        let privacy = documents.iter().any(|d| d == "privacy");
        let what = match (terms, privacy) {
            (true, true) => "unsere Nutzungsbedingungen und unsere Datenschutzerklärung",
            (true, false) => "unsere Nutzungsbedingungen",
            _ => "unsere Datenschutzerklärung",
        };
        let url = if terms {
            format!("{}/nutzungsbedingungen", self.base_url)
        } else {
            format!("{}/datenschutz", self.base_url)
        };
        let next = if requires_acceptance {
            "Beim nächsten Öffnen von Klar bitten wir dich, den neuen Nutzungsbedingungen zuzustimmen. Bist du \
             nicht einverstanden, kannst du dein Konto in den Einstellungen löschen und vorher deine Daten \
             exportieren."
        } else {
            "Du musst nichts tun."
        };

        let text = format!(
            "Wir haben {} geändert\n\nDas ist neu:\n{}\n\n{}\n\nDie vollständige Fassung: {}",
            what, summary, next, url
        );
        // Escaped, and its line breaks (the notice files are written as short
        // paragraphs and lists) kept.
        let escaped = summary
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('\n', "<br>");
        let html = render_html_email(
            &format!("Wir haben {} geändert", what),
            &format!("Das ist neu: {}", escaped),
            "Vollständige Fassung lesen",
            &url,
            next,
        );

        self.send(to_email, "Aktualisierte Bedingungen bei Klar", &text, &html).await
    }

    /// Send password reset link
    pub async fn send_password_reset(&self, to_email: &str, token: &str) -> Result<(), EmailError> {
        // Build the link the user clicks to reset their password.
        let reset_url = format!("{}/reset-password?token={}", self.base_url, token);

        // Plain-text fallback body, for mail clients that don't render HTML.
        let text = format!(
            "Passwort zuruecksetzen\n\n\
             Klicke auf den folgenden Link, um dein Passwort zurueckzusetzen:\n\n\
             {}\n\n\
             Der Link ist 1 Stunde gueltig.\n\n\
             Wenn du diese Anfrage nicht gestellt hast, ignoriere diese E-Mail.",
            reset_url
        );

        // HTML body, built from the shared template above.
        let html = render_html_email(
            "Setze dein Passwort bei Klar zurueck",
            "Du hast angefragt, dein Passwort bei Klar zurueckzusetzen.",
            "Passwort zuruecksetzen",
            &reset_url,
            "Der Link ist 1 Stunde gueltig. Wenn du diese Anfrage nicht gestellt hast, ignoriere diese E-Mail.",
        );

        self.send(to_email, "Passwort zuruecksetzen bei Klar", &text, &html).await
    }

    /// Send an email with both plain-text and HTML alternatives, with the
    /// configured Reply-To.
    async fn send(&self, to: &str, subject: &str, text: &str, html: &str) -> Result<(), EmailError> {
        self.send_with(to, subject, text, html, self.reply_to.as_deref()).await
    }

    async fn send_with(&self, to: &str, subject: &str, text: &str, html: &str, reply_to: Option<&str>) -> Result<(), EmailError> {
        // Dispatch on the configured transport; each provider builds and sends the
        // message differently, but all three end up either Ok(()) or an EmailError.
        match &self.transport {
            Transport::Smtp(mailer) => {
                // Used for both Local (MailHog) and Ionos: build a standard multipart
                // email (plain text + HTML alternative) and hand it to lettre's SMTP client.
                let mut builder = Message::builder()
                    .from(self.from_address.parse().map_err(|e| EmailError(format!("Invalid from: {}", e)))?)
                    .to(to.parse().map_err(|e| EmailError(format!("Invalid to: {}", e)))?);
                if let Some(reply_to) = reply_to {
                    builder = builder.reply_to(reply_to.parse().map_err(|e| EmailError(format!("Invalid reply-to: {}", e)))?);
                }
                let email = builder
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
                // Resend's own client type handles building and sending the request.
                let mut email = CreateEmailBaseOptions::new(
                    self.from_address.as_str(),
                    [to],
                    subject,
                )
                .with_html(html)
                .with_text(text);
                if let Some(reply_to) = reply_to {
                    email = email.with_reply(reply_to);
                }

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
                // characters; both of ours already clear that easily.
                let mut payload = serde_json::json!({
                    "from": { "email": self.from_address },
                    "to": [{ "email": to }],
                    "subject": subject,
                    "text": text,
                    "html": html,
                    "project_id": project_id,
                });
                if let Some(reply_to) = reply_to {
                    payload["additional_headers"] = serde_json::json!([{ "key": "Reply-To", "value": reply_to }]);
                }

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

                // reqwest doesn't turn non-2xx responses into an Err on its own, so
                // this checks the status explicitly and surfaces the response body
                // (Scaleway's error details) in the returned EmailError.
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
