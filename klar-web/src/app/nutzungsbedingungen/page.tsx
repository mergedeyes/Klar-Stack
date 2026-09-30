import Link from "next/link";
import { ArrowLeft } from "lucide-react";

export default function NutzungsbedingungenPage() {
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
          <span className="font-semibold">Nutzungsbedingungen</span>
        </div>
      </header>

      <div className="mx-auto max-w-2xl px-4 py-10 text-sm leading-relaxed">
        <section className="mb-6">
          <h2 className="mb-2 font-semibold">1. Geltungsbereich</h2>
          <p>
            Diese Nutzungsbedingungen regeln die Nutzung von Klar
            („Klar&quot;, „wir&quot;, „uns&quot;), einem Dienst von Jan Motulla, Benzstr. 1,
            88250 Weingarten, Deutschland. Mit der Registrierung eines
            Kontos akzeptierst du diese Nutzungsbedingungen. Informationen zur
            Verarbeitung personenbezogener Daten findest du in der{" "}
            <a href="/datenschutz" className="underline">
              Datenschutzerklärung
            </a>
            .
          </p>
        </section>

        <section className="mb-6">
          <h2 className="mb-2 font-semibold">2. Registrierung und Mindestalter</h2>
          <p>
            Du musst mindestens 16 Jahre alt sein, um ein Konto bei Klar zu
            erstellen. Die Angaben bei der Registrierung müssen wahrheitsgemäß
            sein. Du bist für die Geheimhaltung deines Passworts und für alle
            Aktivitäten unter deinem Konto verantwortlich.
          </p>
        </section>

        <section className="mb-6">
          <h2 className="mb-2 font-semibold">3. Deine Inhalte</h2>
          <p className="mb-2">
            Du behältst alle Rechte an den Fotos, Texten und Nachrichten, die
            du auf Klar veröffentlichst. Damit wir den Dienst technisch
            bereitstellen können (z. B. Speicherung, Anzeige in Feeds,
            Auslieferung über unser CDN), räumst du uns ein einfaches,
            nicht-exklusives, auf die Dauer deiner Nutzung befristetes Recht
            ein, diese Inhalte zu speichern, zu verarbeiten und innerhalb von
            Klar anzuzeigen. Dieses Recht endet mit der Löschung des jeweiligen
            Inhalts bzw. deines Kontos.
          </p>
          <p>
            Du darfst nur Inhalte hochladen, an denen du die erforderlichen
            Rechte besitzt, und keine Inhalte, die Rechte Dritter verletzen.
          </p>
        </section>

        <section className="mb-6">
          <h2 className="mb-2 font-semibold">4. Verbotene Inhalte und Verhalten</h2>
          <p className="mb-2">
            Klar soll ein sicherer Ort für alle sein, unabhängig von Herkunft,
            Hautfarbe, Religion, Geschlecht, sexueller Orientierung,
            geschlechtlicher Identität, Behinderung oder Alter. Deshalb gehen
            wir gegen Hass, Gewalt, Sexualisierung und Belästigung
            konsequenter vor als das Strafrecht allein verlangt.
          </p>
          <p className="mb-2">Bei der Nutzung von Klar ist insbesondere untersagt:</p>
          <ul className="list-disc pl-5 space-y-1">
            <li>
              Inhalte, die gegen geltendes Recht verstoßen (u. a.
              Volksverhetzung, Gewaltdarstellungen, Darstellungen sexuellen
              Missbrauchs von Minderjährigen — hierzu gilt eine Null-Toleranz-
              Politik und wir behalten uns vor, Behörden zu informieren)
            </li>
            <li>Belästigung, Mobbing, Bedrohung oder Stalking anderer Nutzer</li>
            <li>
              Sexuelle Belästigung, also unerwünschte sexuelle Äußerungen oder
              Inhalte, die sich gegen eine bestimmte Person richten
            </li>
            <li>
              Pornografische oder sexuell explizite Inhalte; nicht gemeint ist
              Nacktheit in Kunst, Aufklärung oder beim Stillen
            </li>
            <li>
              Drastische Gewaltdarstellungen ohne dokumentarischen oder
              aufklärenden Zusammenhang und jede Verherrlichung von Gewalt
            </li>
            <li>Inhalte, die zu Selbstverletzung oder Suizid ermutigen oder dazu anleiten</li>
            <li>
              Hassrede und Entmenschlichung: Inhalte, die Menschen wegen ihrer
              Herkunft, Hautfarbe, Religion, Nationalität, ihres Geschlechts,
              ihrer sexuellen Orientierung, ihrer geschlechtlichen Identität,
              einer Behinderung oder ihres Alters herabwürdigen oder ihnen das
              Menschsein absprechen, etwa indem sie mit Tieren, Krankheiten
              oder Ungeziefer gleichgesetzt werden — auch wenn der Inhalt
              nicht strafbar ist
            </li>
            <li>
              Verherrlichung, Verharmlosung oder Rechtfertigung des
              Nationalsozialismus, des Faschismus oder ihrer Verbrechen,
              einschließlich der Leugnung oder Verharmlosung des Holocaust,
              sowie das Verwenden von Kennzeichen verbotener oder
              verfassungswidriger Organisationen
            </li>
            <li>
              Werbung für, Unterstützung von oder Anwerbung für extremistische
              Organisationen, gleich welcher politischen oder religiösen
              Richtung. Extremistisch sind insbesondere Organisationen, die das
              Bundesamt oder ein Landesamt für Verfassungsschutz als gesichert
              extremistisch einstuft, verbotene Vereinigungen sowie
              terroristische Organisationen
            </li>
            <li>Terroristische Inhalte und die Androhung schwerer Gewalt</li>
            <li>Verbreitung intimer Aufnahmen einer Person ohne deren Einwilligung</li>
            <li>Betrug, Betrugsversuche und Phishing</li>
            <li>Angebot von oder Handel mit illegalen Waren, etwa Drogen oder Waffen</li>
            <li>Identitätsdiebstahl oder Vortäuschen falscher Identitäten</li>
            <li>Spam, automatisierte Massen-Registrierungen oder Bots</li>
            <li>
              Versuche, die Sicherheit des Dienstes zu umgehen oder zu
              beeinträchtigen (u. a. Reverse Engineering, unautorisierte
              Zugriffe)
            </li>
          </ul>
          <p className="mt-2">
            Politische Meinungen jeder Richtung sind auf Klar willkommen, auch
            zugespitzte, solange sie diese Grenzen einhalten. Die Regeln
            gegen Extremismus gelten für jede Richtung gleichermaßen.
          </p>
        </section>

        <section className="mb-6">
          <h2 className="mb-2 font-semibold">5. Meldung von Inhalten und Maßnahmen</h2>
          <p>
            Wenn du auf Beiträge, Kommentare oder Profile stößt, die gegen
            diese Nutzungsbedingungen verstoßen, kannst du sie direkt in der
            App über die Melden-Funktion melden und dabei einen passenden
            Grund auswählen (u. a. Spam, Belästigung, Hassrede oder
            Entmenschlichung, Extremismus, Gewaltdarstellung, Terrorismus oder
            Gewaltandrohung, Selbstverletzung, sexuelle Inhalte, intime
            Aufnahmen ohne Einwilligung, Darstellung sexuellen Missbrauchs von
            Minderjährigen, Betrug, illegale Waren, Identitätsdiebstahl oder
            sonstige Verstöße). Je nach gewähltem Grund kann der gemeldete
            Inhalt automatisch vorübergehend ausgeblendet oder mit einem
            Warnhinweis versehen werden, bis unser Team die Meldung geprüft
            hat. Wir behalten uns vor, Inhalte, die gegen diese
            Nutzungsbedingungen verstoßen, zu entfernen und Konten zu sperren
            oder zu löschen, soweit dies zur Wahrung berechtigter Interessen
            oder zur Erfüllung rechtlicher Pflichten erforderlich ist. Über
            jede solche Maßnahme informieren wir dich mit einer Begründung in
            der App und per E-Mail; innerhalb von sechs Monaten kannst du dort
            widersprechen, und ein Mitglied unseres Teams prüft den
            Widerspruch. Wer einen Inhalt meldet, erfährt, ob die Meldung zu
            einer Maßnahme geführt hat. Für
            Anliegen, die sich nicht über die Melden-Funktion abdecken lassen,
            erreichst du uns unter{" "}
            <a href="mailto:kontakt@klarsocial.eu" className="underline">
              kontakt@klarsocial.eu
            </a>
            .
          </p>
        </section>

        <section className="mb-6">
          <h2 className="mb-2 font-semibold">6. Verfügbarkeit</h2>
          <p>
            Klar befindet sich in aktiver Entwicklung. Wir bemühen uns um einen
            stabilen Betrieb, können jedoch keine ununterbrochene Verfügbarkeit
            garantieren. Wartungsarbeiten, Ausfälle oder Änderungen am
            Funktionsumfang sind möglich.
          </p>
        </section>

        <section className="mb-6">
          <h2 className="mb-2 font-semibold">7. Haftung</h2>
          <p>
            Wir haften unbeschränkt für Vorsatz und grobe Fahrlässigkeit sowie
            nach den Vorschriften des Produkthaftungsgesetzes, bei Verletzung
            von Leben, Körper oder Gesundheit. Für leicht fahrlässige
            Verletzung wesentlicher Vertragspflichten (Kardinalpflichten)
            haften wir beschränkt auf den vorhersehbaren, vertragstypischen
            Schaden. Im Übrigen ist die Haftung für leichte Fahrlässigkeit
            ausgeschlossen.
          </p>
        </section>

        <section className="mb-6">
          <h2 className="mb-2 font-semibold">8. Kündigung</h2>
          <p>
            Du kannst dein Konto jederzeit in den Einstellungen löschen. Wir
            können Konten bei Verstößen gegen diese Nutzungsbedingungen
            sperren oder löschen. Bei schwerwiegenden Verstößen kann dies ohne
            vorherige Ankündigung erfolgen.
          </p>
          <p className="mt-2">
            Entfernt unser Team einen Inhalt von dir wegen eines Verstoßes,
            ordnet es ihn einer der folgenden Arten zu. Jede Art hat ein
            festes Kriterium und eine feste Punktzahl (in Klammern), die dein
            Konto dafür erhält; welche Art und welches Kriterium zutreffen,
            steht in der Begründung, die du erhältst:
          </p>
          <ul className="mt-2 list-disc space-y-1 pl-5">
              <li>
                <strong>Spam:</strong> Spam (5); Massenspam (20); Bot- oder reines Spam-Konto, nicht aber ein von Dritten übernommenes Konto (100)
              </li>
              <li>
                <strong>Belästigung:</strong> Einzelne Beleidigung (5); Gezielte oder wiederholte Belästigung (20); Drohung, Stalking oder Veröffentlichung privater Daten (40)
              </li>
              <li>
                <strong>Hassrede:</strong> Abwertende Verallgemeinerung über eine Gruppe (20); Entmenschlichung oder Hetze gegen eine Gruppe (40)
              </li>
              <li>
                <strong>Extremismus:</strong> Verherrlichung von NS/Faschismus, verbotene Symbole (60); Werbung für eine extremistische Organisation, Holocaustleugnung (100)
              </li>
              <li>
                <strong>Gewalt:</strong> Drastische Gewaltdarstellung ohne Einordnung (20); Verherrlichung von Gewalt (40)
              </li>
              <li>
                <strong>Terrorismus:</strong> Terroristische Propaganda (100); Konkrete Androhung eines Anschlags oder schwerer Gewalt (100)
              </li>
              <li>
                <strong>Selbstverletzung:</strong> Eigene Krise (0); Aufforderung oder Anleitung zu Selbstverletzung (40)
              </li>
              <li>
                <strong>Sexuelle Inhalte:</strong> Sexuell expliziter Inhalt (20); Sexuelle Belästigung (60)
              </li>
              <li>
                <strong>Intime Aufnahmen:</strong> Intime Aufnahmen ohne Einwilligung (40); Erpressung mit intimen Aufnahmen (100)
              </li>
              <li>
                <strong>Missbrauchsdarstellungen:</strong> Darstellung sexuellen Missbrauchs von Minderjährigen (100)
              </li>
              <li>
                <strong>Betrug:</strong> Betrugsversuch (20); Phishing (40)
              </li>
              <li>
                <strong>Illegale Waren:</strong> Angebot illegaler Waren (40)
              </li>
              <li>
                <strong>Identität:</strong> Nicht gekennzeichnete Parodie (5); Täuschender Identitätsdiebstahl (20)
              </li>
              <li>
                <strong>Sonstiges:</strong> Sonstiger Verstoß (5)
              </li>
          </ul>
          <p className="mt-2">
            Punkte verfallen je nach Höhe: 5 Punkte nach 90 Tagen, 20 Punkte
            nach 180 Tagen, 40 und 60 Punkte nach einem Jahr; 100 Punkte
            verfallen nicht. Abgelaufene Punkte zählen nicht mehr. Ab
            dem dritten Verstoß aus demselben Grund innerhalb von 30 Tagen
            zählen die Punkte eineinhalbfach. Ab 25 Punkten kommt eine
            Verwarnung in Betracht, ab 50 eine Sperrung für 7 Tage, ab 75 für
            30 Tage und bei 100 Punkten eine dauerhafte Sperrung; einer
            Sperrung geht in der Regel eine Verwarnung voraus. Verstöße mit
            100 Punkten, etwa terroristische Inhalte oder Werbung für
            extremistische Organisationen, können auch ohne vorherige
            Verwarnung zur dauerhaften Sperrung führen. Die Punkte
            sind ein Anhaltspunkt: Jede Verwarnung und Sperrung entscheidet ein
            Mitglied unseres Teams im Einzelfall, unter Berücksichtigung von
            Anzahl, Schwere und Umständen der Verstöße. Während einer Sperrung
            kannst du Klar nur lesen; dein Profil und deine Inhalte sind für
            andere nicht sichtbar. Deine Daten exportieren, dein Konto löschen
            und der Entscheidung widersprechen kannst du weiterhin. Ein
            dauerhaft gesperrtes Konto löschen wir nach Ablauf der
            Widerspruchsfrist von sechs Monaten mit allen Inhalten, solange
            kein Widerspruch offen ist; zwei Wochen vorher erinnern wir dich
            per E-Mail. Deinen Punktestand siehst du jederzeit unter
            „Moderation“ in den Einstellungen.
          </p>
        </section>

        <section className="mb-6">
          <h2 className="mb-2 font-semibold">9. Änderungen dieser Nutzungsbedingungen</h2>
          <p>
            Wir können diese Nutzungsbedingungen ändern, um sie an rechtliche
            oder technische Entwicklungen anzupassen. Über Änderungen
            informieren wir dich mit einem Hinweis in der App, der kurz
            zusammenfasst, was neu ist, und bei bestätigter E-Mail-Adresse
            zusätzlich per E-Mail. Geänderten Nutzungsbedingungen musst du
            beim nächsten Öffnen von Klar zustimmen, um Klar weiter zu
            nutzen; bist du nicht einverstanden, kannst du deine Daten
            exportieren und dein Konto löschen.
          </p>
        </section>

        <section className="mb-6">
          <h2 className="mb-2 font-semibold">10. Schlussbestimmungen</h2>
          <p>
            Es gilt das Recht der Bundesrepublik Deutschland. Sollten einzelne
            Bestimmungen dieser Nutzungsbedingungen unwirksam sein, bleibt die
            Wirksamkeit der übrigen Bestimmungen unberührt.
          </p>
        </section>

        <p className="text-xs text-muted-foreground">
          Stand: 29.09.2026
        </p>
      </div>
    </div>
  );
}
