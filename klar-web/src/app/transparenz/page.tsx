import Link from "next/link";
import { ArrowLeft } from "lucide-react";

export default function TransparenzPage() {
  return (
    <div className="flex-1 bg-background">
      <header className="sticky top-0 z-10 border-b border-border bg-background/80 backdrop-blur">
        <div className="mx-auto flex h-14 max-w-2xl items-center gap-3 px-4">
          <Link
            href="/"
            aria-label="Back"
            className="inline-flex h-9 w-9 items-center justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground"
          >
            <ArrowLeft size={20} />
          </Link>
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
            Dein Feed ist chronologisch — bewusst ohne Algorithmus. Du siehst
            Beiträge der Menschen, denen du folgst, in der Reihenfolge, in der
            sie gepostet wurden. Kein Beitrag wird dort nach oben sortiert,
            weil er mehr Interaktionen bekommt, und daran ändert sich nichts.
          </p>
          <p className="mb-2">
            Nur die Entdecken-Seite, auf der du Beiträge von Menschen findest,
            denen du noch nicht folgst, soll künftig zu dir passende Beiträge
            weiter oben zeigen. Noch ist auch sie chronologisch. Wie das
            funktioniert und welche Daten dafür anfallen, steht unten unter
            „Einträge für die Entdecken-Seite“.
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
              Nachrichten sind nur für die beiden Gesprächspartner sichtbar —
              außer eine von beiden meldet eine Nachricht: Dann kann unser
              Team sie und die zehn Nachrichten davor lesen (siehe
              „Moderation und Sicherheit“).
            </p>
          </div>

          <div className="mb-4">
            <h3 className="mb-1 font-semibold">Benachrichtigungen</h3>
            <p>
              Wenn dir jemand folgt oder deinen Beitrag/Kommentar liked oder
              kommentiert, wird das kurz gespeichert (wer, was, wann), damit
              du es in deiner Benachrichtigungsliste siehst. Diese Ereignisse
              werden dir außerdem in Echtzeit zugestellt (siehe „Echtzeit-
              Benachrichtigungen“ unten). Gelesene Benachrichtigungen löschen
              wir nach 90 Tagen, ungelesene nach einem Jahr.
            </p>
          </div>

          <div className="mb-4">
            <h3 className="mb-1 font-semibold">Einträge für die Entdecken-Seite</h3>
            <p>
              Damit die Entdecken-Seite dir künftig passende Beiträge zeigen
              kann, speichern wir, welche Beiträge du likest, nicht mehr
              likest oder kommentierst und welche Kommentare du likest oder
              nicht mehr likest — mit
              Zeitpunkt. Was du dir nur ansiehst, speichern wir nicht. Dein
              Feed nutzt diese Einträge nie. In den Einstellungen kannst du das
              mit „Personalised Discovery“ abschalten; dann löschen wir sofort
              alles, was gespeichert war. Sonst löschen wir die Einträge
              monatsweise, sobald sie zwölf Monate alt sind. Ein früheres
              Protokoll dieser Art hatten wir im Oktober 2026 gelöscht, weil es
              keinen Zweck hatte; dieses dient nur der Entdecken-Seite (Details
              in der{" "}
              <Link href="/datenschutz#discovery" className="underline">Datenschutzerklärung</Link>).
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
            Dabei werden nur die interne Nutzer-ID, der Benutzername, der
            Anzeigename und die Profilbild-URL der auslösenden Person
            mitübertragen (technisch notwendig, um die Benachrichtigung
            zusammenzustellen) — <strong>keine E-Mail-Adressen</strong>. Die
            Übertragung ist TLS-verschlüsselt, und die Daten werden dort nicht
            dauerhaft gespeichert.
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
            Die Datenbank, Bilder, Videos und das Hosting der Anwendung
            laufen über Bunny.net mit einem deutschen Rechenzentrum. Details
            zu allen eingesetzten Dienstleistern (einschließlich des
            E-Mail-Versands) findest du in der{" "}
            <Link href="/datenschutz" className="underline">Datenschutzerklärung</Link>.
          </p>
          <p className="mt-2">
            Jede Nacht sichern wir die Datenbank, ebenfalls bei Bunny.net in
            Deutschland; jede Sicherung wird nach 14 Tagen automatisch
            gelöscht. Ob die Sicherung geklappt hat, meldet unser Server an
            Healthchecks.io (EU) — nur ein „hat funktioniert“ oder „ist
            fehlgeschlagen“, ohne jegliche Nutzerdaten. So merken wir sofort,
            wenn etwas nicht stimmt.
          </p>
        </section>

        <section className="mb-8">
          <h2 className="mb-2 text-lg font-semibold">Moderation und Sicherheit</h2>
          <p className="mb-2">
            Klar soll ein sicherer Ort für alle sein. Was verboten ist, steht
            in Abschnitt 4 der{" "}
            <Link href="/nutzungsbedingungen" className="underline">Nutzungsbedingungen</Link>;
            hier steht, wie wir damit umgehen.
          </p>
          <div className="mb-4">
            <h3 className="mb-1 font-semibold">Meldungen</h3>
            <p>
              Jeder kann Beiträge, Kommentare, Direktnachrichten und Profile
              melden, mit einer bestätigten E-Mail-Adresse. Rechtswidrige
              Inhalte kann außerdem jede Person ohne Konto über das Formular{" "}
              <Link href="/notices" className="underline">„Rechtswidrige Inhalte melden“</Link>{" "}
              melden. Eine einzelne Meldung entfernt nichts, mit zwei Ausnahmen:
              Bei Darstellungen sexuellen Missbrauchs von Minderjährigen und
              intimen Aufnahmen ohne Einwilligung wird der Inhalt sofort
              ausgeblendet, weil jeder weitere Aufruf der gezeigten Person
              schadet. Bei Gewalt, Selbstverletzung, sexuellen Inhalten und
              Terrorismus erscheint er bis zur Prüfung hinter einem
              Warnhinweis. Das gilt nur für Meldungen von Konten, die
              mindestens einen Tag alt sind und deren Meldungen dieser Art nicht
              wiederholt unbegründet waren — sonst könnte jemand mit einem
              neuen Konto beliebige Beiträge verschwinden lassen. Alles andere
              bleibt sichtbar, bis ein Mensch aus unserem Team entschieden hat.
              Mehrere Meldungen zum selben Inhalt entscheiden wir gemeinsam, und
              jede meldende Person erfährt das Ergebnis; stellen wir keinen
              Verstoß fest, kann sie einmal um erneute Prüfung bitten. Bei
              Missbrauchsdarstellungen, intimen Aufnahmen und Terrorismus
              benachrichtigt Klar unser Team sofort per E-Mail; die E-Mail
              enthält keine Inhalte und keine Namen.
            </p>
          </div>
          <div className="mb-4">
            <h3 className="mb-1 font-semibold">Gemeldete Direktnachrichten</h3>
            <p>
              Meldest du eine Nachricht, sichern wir sie und die zehn
              Nachrichten davor, damit unser Team versteht, worauf sie
              antwortete — weitere Nachrichten des Gesprächs sehen wir nicht.
              Die Kopie bleibt, auch wenn die verfassende Person die Nachricht
              oder ihr Konto löscht; sonst könnte sie Belästigung einfach
              verschwinden lassen. Jeder Blick unseres Teams darauf wird
              protokolliert.
            </p>
          </div>
          <div className="mb-4">
            <h3 className="mb-1 font-semibold">Entscheidungen und Punkte</h3>
            <p>
              Entfernen wir einen Inhalt, ordnen wir ihn einer festen Art von
              Verstoß zu, jede mit einem schriftlichen Kriterium und einer
              festen Punktzahl (Liste in Abschnitt 8 der
              Nutzungsbedingungen). Weichen wir dabei vom Meldegrund ab oder
              vergeben keine Punkte, müssen wir das intern begründen. Die
              Punkte verfallen je nach Schwere nach 90 Tagen bis einem Jahr;
              ab dem dritten Verstoß aus demselben Grund innerhalb von 30
              Tagen zählen sie eineinhalbfach. Der Punktestand schlägt eine
              Verwarnung oder Sperrung vor, entscheiden tut immer ein Mensch,
              nie ein Automatismus. Zu jeder Entscheidung bekommst du eine
              Begründung in der App und per E-Mail und kannst sechs Monate
              lang widersprechen; deinen Punktestand siehst du unter
              Einstellungen → Moderation. Statt eines ganzen Kontos können wir
              auch nur einzelne Profilangaben entfernen, etwa ein Profilbild
              oder einen Benutzernamen, der eine andere Person nachahmt.
            </p>
          </div>
          <div className="mb-4">
            <h3 className="mb-1 font-semibold">Entfernen heißt erst einmal: unsichtbar</h3>
            <p>
              Was unser Team entfernt, verschwindet sofort für alle, wird aber
              erst nach Ablauf der sechsmonatigen Widerspruchsfrist endgültig
              gelöscht. Hat dein Widerspruch Erfolg, stellen wir es genau so
              wieder her, wie es war. Antworten anderer auf einen entfernten
              Kommentar bleiben stehen; an seiner Stelle steht „Removed by
              moderation“. Darstellungen sexuellen Missbrauchs von
              Minderjährigen löschen wir dagegen, sobald die Beweissicherung
              abgeschlossen ist.
            </p>
          </div>
          <div className="mb-4">
            <h3 className="mb-1 font-semibold">Woher eine Entscheidung kommt</h3>
            <p>
              Die meisten Entscheidungen folgen auf eine Meldung. Unser Team
              kann aber auch von sich aus handeln, etwa bei einem Inhalt, auf
              den es selbst stößt, oder auf Anordnung einer Behörde. Die
              Begründung nennt immer den Anlass, bei einer Anordnung auch die
              Behörde. Jede Entscheidung wird mit Person, Zeitpunkt und Grund
              festgehalten.
            </p>
          </div>
          <div className="mb-4">
            <h3 className="mb-1 font-semibold">Sperrungen</h3>
            <p>
              Ein gesperrtes Konto kann Klar nur noch lesen; Profil und Inhalte
              sind für andere nicht sichtbar. Exportieren, Konto löschen und
              widersprechen geht weiterhin. Ein dauerhaft gesperrtes Konto
              löschen wir nach Ablauf der sechsmonatigen Widerspruchsfrist,
              nach einer Erinnerung zwei Wochen vorher.
            </p>
          </div>
          <div className="mb-4">
            <h3 className="mb-1 font-semibold">Übernommene Konten und Bots</h3>
            <p>
              Um übernommene Konten und Bots zu erkennen, achten wir auf
              auffällige Muster in Daten, die ohnehin gespeichert sind, etwa
              sehr viele Beiträge in wenigen Minuten oder immer denselben
              Text. IP-Adressen oder Geräte erfassen wir dafür nicht. Ein
              Mensch prüft dann die jüngste Aktivität des Kontos; von
              Direktnachrichten sieht er nur die Anzahl, nie den Inhalt. Jede
              solche Prüfung wird mit Grund protokolliert. Sieht ein Konto
              übernommen aus, sperren wir es vorsorglich und schicken dir einen
              Link, mit dem ein neues Passwort es wieder freigibt.
            </p>
          </div>
          <div className="mb-4">
            <h3 className="mb-1 font-semibold">Behörden</h3>
            <p>
              Deutet ein Inhalt auf eine Straftat hin, die Leben oder
              Sicherheit von Menschen gefährdet, etwa eine Anschlagsdrohung
              oder Missbrauchsdarstellungen, müssen wir das den Behörden
              melden (Art. 18 Digital Services Act). Andere möglicherweise
              strafbare Inhalte, etwa Holocaustleugnung, können wir ebenfalls
              zur Anzeige bringen. In beiden Fällen sichern wir den Inhalt als
              Beweis. Darüber hinaus geben wir Daten nur heraus, wenn eine
              Behörde sie rechtmäßig anfordert.
            </p>
          </div>
        </section>

        <section className="mb-8">
          <h2 className="mb-2 text-lg font-semibold">Löschung</h2>
          <p>
            Löschst du dein Konto in den Einstellungen, werden dein Profil,
            deine Beiträge, Kommentare, Likes und Direktnachrichten sofort
            entfernt — Nachrichten, die du anderen geschickt hast, auch aus
            deren Chat. Dort stehst du dann als „Deleted User“, ohne Link zu
            einem Profil.
            Ausnahme: Wird etwas von dir wegen eines möglicherweise
            strafbaren Inhalts gemeldet, sichern wir sofort eine Kopie davon
            als Beweismittel, dazu jede Änderung, solange die Meldung offen
            ist — nur diesen einen Inhalt, getrennt von allem anderen, nur
            für die Moderation einsehbar, und jeder Blick darauf wird
            protokolliert. Diese Kopie bleibt, auch wenn du den Inhalt oder
            dein Konto löschst. War die Meldung unbegründet, löschen wir die
            Kopie sofort, sonst nach sechs Monaten, außer eine Behörde
            braucht sie länger.
            Punkte aus Verstößen und die dazu gespeicherte Kopie des
            entfernten Inhalts löschen wir, sobald die Punkte verfallen.
            Meldungen löschen wir sechs Monate nach unserer Entscheidung,
            Entscheidungen nach drei Jahren — beides nur, wenn nichts mehr
            darauf beruht. Ein Konto, dessen E-Mail-Adresse nach 30 Tagen
            noch nicht bestätigt ist, löschen wir nach einer Erinnerung eine
            Woche vorher.
            In den nächtlichen Sicherungen können deine Daten noch bis zu 14
            Tage liegen, bevor diese automatisch gelöscht werden.
            Serverseitige Zugriffsprotokolle laufen unabhängig davon ohnehin
            nach 3 Tagen ab.
          </p>
        </section>

        <section className="mb-8">
          <h2 className="mb-2 text-lg font-semibold">Deine Kontrolle</h2>
          <p>
            Unter <strong>Einstellungen → Download your data</strong> kannst
            du jederzeit alle zu dir gespeicherten Daten als ZIP-Archiv
            exportieren (JSON-Datei plus deine hochgeladenen Bilder). Kontolöschung findest du an derselben Stelle.
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
          Stand: 29.09.2026
        </p>
      </div>
    </div>
  );
}
