import Link from "next/link";
import { SmartBackButton } from "@/components/SmartBackButton";

export default function TransparenzPage() {
  return (
    <div className="min-h-screen bg-background">
      <header className="sticky top-0 z-10 border-b border-border bg-background/80 backdrop-blur">
        <div className="mx-auto flex h-14 max-w-2xl items-center gap-3 px-4">
          <SmartBackButton aria-label="Back" />
          <span className="font-semibold">Transparenz</span>
        </div>
      </header>

      <div className="mx-auto max-w-2xl px-4 py-10 text-sm leading-relaxed">
        <h1 className="mb-2 text-2xl font-bold">Transparenz: Wie Klar funktioniert</h1>
        <p className="mb-6 text-muted-foreground">
          Diese Seite erklärt in einfacher Sprache, wie Klar technisch
          funktioniert und welche Daten dabei anfallen. Sie ersetzt nicht die{" "}
          <Link href="/datenschutz" className="underline">Datenschutzerklärung</Link>{" "}
          — die bleibt das rechtlich maßgebliche Dokument. Diese Seite soll
          einfach verständlich machen, was dort in juristischer Sprache steht.
        </p>

        <section className="mb-8">
          <h2 className="mb-2 text-lg font-semibold">So funktioniert Klar</h2>
          <p className="mb-2">
            Klar ist ein chronologischer Feed — bewusst ohne Algorithmus. Du
            siehst Beiträge der Menschen, denen du folgst, in der Reihenfolge,
            in der sie gepostet wurden. Kein Beitrag wird nach oben sortiert,
            weil er mehr Interaktionen bekommt.
          </p>
          <p>
            Direktnachrichten sind nur zwischen Nutzern möglich, die sich
            gegenseitig folgen. Alle anderen Interaktionen (Kommentare, Likes)
            sind öffentlich sichtbar, sofern das jeweilige Profil bzw. der
            Beitrag es ist.
          </p>
        </section>

        <section className="mb-8">
          <h2 className="mb-3 text-lg font-semibold">Welche Daten wir speichern</h2>

          <div className="mb-4">
            <h3 className="mb-1 font-semibold">Kontodaten</h3>
            <p>
              Benutzername, E-Mail-Adresse, Passwort (als Argon2-Hash — wir
              können dein tatsächliches Passwort nicht einsehen), optionaler
              Anzeigename, Bio und Profilbild.
            </p>
          </div>

          <div className="mb-4">
            <h3 className="mb-1 font-semibold">Inhalte</h3>
            <p>
              Fotos, Bildunterschriften, Kommentare und Likes. Hochgeladene
              Fotos werden serverseitig neu verarbeitet: Wir entfernen dabei
              automatisch alle EXIF-Metadaten (u. a. Standortdaten,
              Geräteinformationen, Aufnahmezeitpunkt) und erzeugen daraus drei
              Größen (Vorschau, mittel, Original) für schnelles Laden.
            </p>
          </div>

          <div className="mb-4">
            <h3 className="mb-1 font-semibold">Sozialer Graph</h3>
            <p>
              Wem du folgst und wer dir folgt, sowie blockierte Nutzer (nur für
              dich sichtbar, nicht für die blockierte Person).
            </p>
          </div>

          <div className="mb-4">
            <h3 className="mb-1 font-semibold">Direktnachrichten</h3>
            <p>
              Nachrichteninhalt, Zeitstempel, Lesestatus und Emoji-Reaktionen.
              Nachrichten sind nur für die beiden Gesprächspartner sichtbar.
            </p>
          </div>

          <div className="mb-4">
            <h3 className="mb-1 font-semibold">Benachrichtigungen</h3>
            <p>
              Wenn dir jemand folgt oder deinen Beitrag/Kommentar liked oder
              kommentiert, wird das kurz gespeichert (wer, was, wann), damit
              du es in deiner Benachrichtigungsliste siehst. Diese Ereignisse
              werden dir außerdem in Echtzeit zugestellt (siehe „Echtzeit-
              Benachrichtigungen" unten).
            </p>
          </div>

          <div className="mb-4">
            <h3 className="mb-1 font-semibold">Nutzungsereignisse</h3>
            <p>
              Wir protokollieren, welche Beiträge du ansiehst, likest oder
              kommentierst, verknüpft mit deinem Konto (falls eingeloggt).
              Das ist heute die Grundlage für mögliche zukünftige Funktionen
              (z. B. bessere Empfehlungen) — <strong>aktuell beeinflusst das
              nichts</strong>: dein Feed bleibt rein chronologisch, wie oben
              beschrieben. Wir wollten das trotzdem hier nennen, auch wenn es
              noch keine sichtbare Auswirkung hat.
            </p>
          </div>

          <div className="mb-4">
            <h3 className="mb-1 font-semibold">Technische Daten / Server-Logs</h3>
            <p>
              Beim Aufruf der Anwendung fallen technisch bedingt
              Zugriffsprotokolle an (IP-Adresse, Zeitpunkt, aufgerufene Route,
              Statuscode) — IP-Adressen werden dabei anonymisiert und die
              Protokolle nach spätestens 3 Tagen automatisch gelöscht.
            </p>
          </div>
        </section>

        <section className="mb-8">
          <h2 className="mb-2 text-lg font-semibold">Echtzeit-Benachrichtigungen</h2>
          <p>
            Damit Benachrichtigungen sofort ankommen (auch wenn unser Backend
            auf mehreren Servern läuft), werden sie kurzzeitig über einen
            externen, verschlüsselt angebundenen Dienst (Upstash) geleitet.
            Dabei werden Benutzername, Profilbild-URL und E-Mail-Adresse der
            auslösenden Person mitübertragen (technisch notwendig, um die
            Benachrichtigung zusammenzustellen) — die Übertragung ist
            TLS-verschlüsselt, die Daten werden dort nicht dauerhaft
            gespeichert, und deine E-Mail-Adresse wird in der App-Oberfläche
            nirgends angezeigt.
          </p>
        </section>

        <section className="mb-8">
          <h2 className="mb-2 text-lg font-semibold">Cookies und lokaler Speicher</h2>
          <p className="mb-2">
            Klar setzt <strong>keine</strong> Tracking-, Analyse- oder
            Werbe-Cookies ein. Für den Login verwenden wir zwei Mechanismen
            nebeneinander:
          </p>
          <ul className="list-disc pl-5 space-y-1">
            <li>
              <strong>Lokaler Speicher (localStorage)</strong> im Browser:
              enthält deinen Zugriffs- und Refresh-Token. Das ist der
              primäre Mechanismus, da manche Browser Cookies über
              verschiedene Domains hinweg (klarsocial.eu / klarsocial.de)
              blockieren.
            </li>
            <li>
              <strong>HttpOnly-Cookies</strong> als zusätzliche, ergänzende
              Absicherung, mit denselben Tokens — nicht per JavaScript
              auslesbar, technisch zum Betrieb des Logins erforderlich.
            </li>
          </ul>
        </section>

        <section className="mb-8">
          <h2 className="mb-2 text-lg font-semibold">Wo deine Daten liegen</h2>
          <p>
            Unsere Datenbank läuft in Frankfurt (Neon, AWS-Region
            eu-central-1). Bilder, Videos und das Hosting der Anwendung
            laufen über Bunny.net mit einem deutschen Rechenzentrum. Details
            zu allen eingesetzten Dienstleistern (einschließlich des
            E-Mail-Versands) findest du in der{" "}
            <Link href="/datenschutz" className="underline">Datenschutzerklärung</Link>.
          </p>
        </section>

        <section className="mb-8">
          <h2 className="mb-2 text-lg font-semibold">Löschung</h2>
          <p>
            Löschst du dein Konto in den Einstellungen, werden dein Profil,
            deine Beiträge, Kommentare und Nachrichten entfernt. Serverseitige
            Zugriffsprotokolle laufen unabhängig davon ohnehin nach 3 Tagen ab.
          </p>
        </section>

        <section className="mb-8">
          <h2 className="mb-2 text-lg font-semibold">Deine Kontrolle</h2>
          <p>
            Unter <strong>Einstellungen → Download your data</strong> kannst
            du jederzeit alle zu dir gespeicherten Daten als JSON-Datei
            exportieren. Kontolöschung findest du an derselben Stelle.
          </p>
        </section>

        <p className="text-xs text-muted-foreground">
          Diese Seite beschreibt den technischen Ist-Zustand nach bestem
          Wissen und wird bei größeren Änderungen aktualisiert. Rechtlich
          verbindlich sind die{" "}
          <Link href="/datenschutz" className="underline">Datenschutzerklärung</Link>{" "}
          und die{" "}
          <Link href="/nutzungsbedingungen" className="underline">Nutzungsbedingungen</Link>.
          <br />
          Stand: 23.07.2026
        </p>
      </div>
    </div>
  );
}
