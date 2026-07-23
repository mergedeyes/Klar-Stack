import { SmartBackButton } from "@/components/SmartBackButton";

export default function DatenschutzPage() {
  return (
    <div className="min-h-screen bg-background">
      <header className="sticky top-0 z-10 border-b border-border bg-background/80 backdrop-blur">
        <div className="mx-auto flex h-14 max-w-2xl items-center gap-3 px-4">
          <SmartBackButton aria-label="Back" />
          <span className="font-semibold">Datenschutzerklärung</span>
        </div>
      </header>

      <div className="mx-auto max-w-2xl px-4 py-10 text-sm leading-relaxed">
        <section className="mb-6">
          <h2 className="mb-2 font-semibold">1. Verantwortlicher</h2>
          <p>
            Jan Motulla
            <br />
            Benzstr. 1
            <br />
            88250 Weingarten
            <br />
            Deutschland
          </p>
        </section>

        <section className="mb-6">
          <h2 className="mb-2 font-semibold">Kontakt</h2>
          <p>
            E-Mail:{" "}
            <a href="mailto:kontakt@klarsocial.eu" className="underline">
              kontakt@klarsocial.eu
            </a>
          </p>
        </section>

        <section className="mb-6">
          <h2 className="mb-2 font-semibold">2. Registrierung und Nutzerkonto</h2>
          <p>
            Bei der Registrierung erheben wir Benutzername, E-Mail-Adresse und
            ein gehashtes Passwort (Argon2 — das Klartext-Passwort wird nicht
            gespeichert). Rechtsgrundlage ist die Erfüllung des
            Nutzungsvertrags (Art. 6 Abs. 1 lit. b DSGVO).
          </p>
        </section>

        <section className="mb-6">
          <h2 className="mb-2 font-semibold">3. Inhalte (Fotos, Beiträge, Nachrichten)</h2>
          <p>
            Hochgeladene Fotos werden serverseitig verarbeitet: Wir entfernen
            automatisch alle EXIF-Metadaten (u. a. Standortdaten, Geräteinfo,
            Aufnahmezeitpunkt), bevor das Bild gespeichert wird. Beiträge,
            Kommentare und Direktnachrichten werden gespeichert, um die
            Kernfunktion des Dienstes bereitzustellen (Art. 6 Abs. 1 lit. b
            DSGVO).
          </p>
        </section>

        <section className="mb-6">
          <h2 className="mb-2 font-semibold">4. Hosting und Auftragsverarbeiter</h2>
          <p className="mb-2">
            Wir setzen folgende Dienstleister ein, mit denen jeweils ein
            Auftragsverarbeitungsvertrag (Art. 28 DSGVO) besteht bzw. bestehen
            sollte:
          </p>
          <ul className="list-disc pl-5 space-y-2">
            <li>
              <strong>Bunny.net</strong> (Hosting der Anwendung, CDN und
              Speicherung von Bild-Dateien). Speicherung erfolgt in einem
              deutschen Rechenzentrum.
            </li>
            <li>
              <strong>Neon</strong> (Datenbank-Hosting, PostgreSQL). Die
              Datenbank läuft in der AWS-Region eu-central-1 (Frankfurt,
              Deutschland).
            </li>
            <li>
              <strong>Scaleway</strong> (Versand von Transaktions-E-Mails, z. B.
              Registrierungsbestätigung und Passwort-Reset, über den Dienst
              „Transactional Email"). Scaleway ist ein französisches
              Unternehmen mit Sitz in der EU; der Versand erfolgt über die
              Region Paris (fr-par). Da es sich um einen EU-Anbieter handelt,
              ist keine Drittlandübermittlung im Sinne von Art. 44 ff. DSGVO
              involviert. [Bitte vor Live-Gang prüfen, ob ein aktueller
              Auftragsverarbeitungsvertrag nach Art. 28 DSGVO mit Scaleway
              abgeschlossen wurde — dies ist unabhängig vom Sitz des
              Anbieters gesetzlich vorgeschrieben.]
            </li>
            <li>
              <strong>Upstash</strong> (kurzzeitige Weiterleitung von
              Echtzeit-Benachrichtigungen, z. B. bei einem Like, Kommentar
              oder neuen Follower, damit diese sofort und über mehrere
              Server hinweg zugestellt werden können — siehe auch unsere{" "}
              <a href="/transparenz" className="underline">Transparenzseite</a>
              {" "}für eine ausführlichere Erklärung). Dabei werden
              Benutzername, Profilbild-URL und E-Mail-Adresse der jeweils
              auslösenden Person übertragen, jedoch nicht dauerhaft bei
              Upstash gespeichert. Upstash ist ein US-amerikanisches
              Unternehmen; die Übermittlung erfolgt auf Grundlage von
              EU-Standardvertragsklauseln (Art. 46 DSGVO) bzw. des
              EU-U.S. Data Privacy Framework. [Bitte vor Live-Gang prüfen, ob
              ein aktueller Auftragsverarbeitungsvertrag inkl. SCC mit
              Upstash abgeschlossen wurde.]
            </li>
          </ul>
        </section>

        <section className="mb-6">
          <h2 className="mb-2 font-semibold">5. Cookies und lokaler Speicher</h2>
          <p>
            Zur Anmeldung verwenden wir Zugriffs- und Refresh-Token, die im
            <code className="mx-1 rounded bg-muted px-1">localStorage</code>
            deines Browsers abgelegt werden, sowie ergänzend Cookies. Dies ist
            technisch erforderlich, um dich eingeloggt zu halten (Art. 6 Abs. 1
            lit. b DSGVO). Es werden keine Tracking- oder Werbe-Cookies
            eingesetzt.
          </p>
        </section>

        <section className="mb-6">
          <h2 className="mb-2 font-semibold">6. Server-Logs</h2>
          <p>
            Beim Aufruf der Anwendung werden technisch bedingt Zugriffsprotokolle
            (IP-Adresse, Zeitpunkt, aufgerufene Route, Statuscode) auf
            Ebene unseres CDN- und Hosting-Anbieters (Bunny.net) verarbeitet,
            um den Betrieb sicherzustellen und Fehler zu erkennen (Art. 6 Abs.
            1 lit. f DSGVO — berechtigtes Interesse am sicheren Betrieb).
            IP-Adressen werden dabei standardmäßig anonymisiert gespeichert.
            Diese Protokolle werden automatisch nach maximal 3 Tagen gelöscht;
            eine darüber hinausgehende, dauerhafte Speicherung findet nicht
            statt.
          </p>
        </section>

        <section className="mb-6">
          <h2 className="mb-2 font-semibold">7. Deine Rechte</h2>
          <p>
            Du hast das Recht auf Auskunft (Art. 15 DSGVO), Berichtigung (Art.
            16), Löschung (Art. 17), Einschränkung der Verarbeitung (Art. 18),
            Datenübertragbarkeit (Art. 20) sowie Widerspruch (Art. 21) gegen
            die Verarbeitung deiner Daten. Für das Auskunftsrecht und die
            Datenübertragbarkeit steht dir in den Einstellungen unter{" "}
            <strong>„Download your data"</strong> ein direkter Selbstbedienungs-Export
            zur Verfügung, der dir alle gespeicherten Daten als JSON-Datei
            bereitstellt. Für alle anderen Anliegen wende dich an{" "}
            <a href="mailto:kontakt@klarsocial.eu" className="underline">
              kontakt@klarsocial.eu
            </a>
            . Außerdem steht dir ein Beschwerderecht bei einer
            Datenschutz-Aufsichtsbehörde zu.
          </p>
        </section>

        <section className="mb-6">
          <h2 className="mb-2 font-semibold">8. Löschung deines Kontos</h2>
          <p>
            Du kannst dein Konto jederzeit in den Einstellungen löschen. Dabei
            werden dein Profil, deine Beiträge, Kommentare und Nachrichten
            gemäß unserer Datenbankstruktur entfernt.
          </p>
        </section>

        <p className="text-xs text-muted-foreground">
          Stand: 23.07.2026
        </p>
      </div>
    </div>
  );
}
