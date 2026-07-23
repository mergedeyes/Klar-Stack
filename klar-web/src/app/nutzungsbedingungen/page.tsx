import Link from "next/link";
import { ArrowLeft } from "lucide-react";

export default function NutzungsbedingungenPage() {
  return (
    <div className="min-h-screen bg-background">
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
            („Klar", „wir", „uns"), einem Dienst von Jan Motulla, Benzstr. 1,
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
          <p className="mb-2">Bei der Nutzung von Klar ist insbesondere untersagt:</p>
          <ul className="list-disc pl-5 space-y-1">
            <li>
              Inhalte, die gegen geltendes Recht verstoßen (u. a.
              Volksverhetzung, Gewaltdarstellungen, Darstellungen sexuellen
              Missbrauchs von Minderjährigen — hierzu gilt eine Null-Toleranz-
              Politik und wir behalten uns vor, Behörden zu informieren)
            </li>
            <li>Belästigung, Mobbing, Bedrohung oder Stalking anderer Nutzer</li>
            <li>Identitätsdiebstahl oder Vortäuschen falscher Identitäten</li>
            <li>Spam, automatisierte Massen-Registrierungen oder Bots</li>
            <li>
              Versuche, die Sicherheit des Dienstes zu umgehen oder zu
              beeinträchtigen (u. a. Reverse Engineering, unautorisierte
              Zugriffe)
            </li>
          </ul>
        </section>

        <section className="mb-6">
          <h2 className="mb-2 font-semibold">5. Meldung von Inhalten und Maßnahmen</h2>
          <p>
            Wenn du auf Inhalte stößt, die gegen diese Nutzungsbedingungen
            verstoßen, kannst du dies an{" "}
            <a href="mailto:kontakt@klarsocial.eu" className="underline">
              kontakt@klarsocial.eu
            </a>{" "}
            melden. Wir behalten uns vor, Inhalte, die gegen diese
            Nutzungsbedingungen verstoßen, zu entfernen und Konten zu sperren
            oder zu löschen, soweit dies zur Wahrung berechtigter Interessen
            oder zur Erfüllung rechtlicher Pflichten erforderlich ist.
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
        </section>

        <section className="mb-6">
          <h2 className="mb-2 font-semibold">9. Änderungen dieser Nutzungsbedingungen</h2>
          <p>
            Wir können diese Nutzungsbedingungen ändern, um sie an rechtliche
            oder technische Entwicklungen anzupassen. Über wesentliche
            Änderungen informieren wir dich in geeigneter Form (z. B. per
            E-Mail oder Hinweis innerhalb der Anwendung).
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
          Stand: 23.07.2026
        </p>
      </div>
    </div>
  );
}
