import Link from "next/link";
import { ArrowLeft } from "lucide-react";

export default function DatenschutzPage() {
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
          <p className="mt-2">
            <strong>Bestätigung der E-Mail-Adresse:</strong> Beiträge,
            Kommentare, Direktnachrichten und Meldungen sind erst möglich,
            wenn du deine E-Mail-Adresse bestätigt hast; E-Mails zu
            Moderationsentscheidungen schicken wir nur an bestätigte Adressen.
            Ein Konto, dessen Adresse 30 Tage nach der Registrierung noch nicht
            bestätigt ist, löschen wir mit allen Daten; eine Woche vorher
            erinnern wir dich mit einem neuen Bestätigungslink. Eine nicht
            bestätigte Adresse kann einer anderen Person gehören, und ein
            Konto, das wir nicht erreichen können, bewahren wir nicht ohne
            Grund auf (Art. 5 Abs. 1 lit. c und e, Art. 6 Abs. 1 lit. b und f
            DSGVO).
          </p>
          <p className="mt-2">
            <strong>Änderungen unserer Bedingungen:</strong> Ändern sich die
            Nutzungsbedingungen oder diese Datenschutzerklärung, zeigen wir dir
            beim nächsten Öffnen von Klar einen Hinweis und schreiben an
            bestätigte E-Mail-Adressen. Wir speichern, wann du den Hinweis
            gesehen oder geänderten Nutzungsbedingungen zugestimmt hast, als
            Nachweis (Art. 6 Abs. 1 lit. b und c DSGVO); das wird mit deinem
            Konto gelöscht.
          </p>
          <p className="mt-2">
            <strong>Schutz bei Verdacht auf fremden Zugriff:</strong> Deutet
            Aktivität darauf hin, dass jemand anderes dein Konto benutzt (etwa
            plötzlich massenhaft Spam), kann unser Team es vorsorglich sperren:
            Du wirst auf allen Geräten abgemeldet und erhältst per E-Mail einen
            Link, mit dem ein neues Passwort das Konto wieder entsperrt. Wir
            speichern dazu, wer wann aus welchem Grund gesperrt hat, wie oft
            ein Link versandt wurde und wann das Konto wieder entsperrt wurde,
            sowie unsere Einschätzung des Vorfalls. Das dient der Sicherheit
            deines Kontos und deiner Kontakte und der Dokumentation von
            Datenschutzverletzungen (Art. 6 Abs. 1 lit. c und f, Art. 33 Abs. 5
            DSGVO). Löschst du dein Konto, bleibt der Vorfall ohne Verknüpfung
            mit dir dokumentiert. Um zu erkennen, ob ein Konto übernommen wurde
            oder automatisiert betrieben wird, wertet unser Team ungewöhnliche
            Aktivität aus bereits gespeicherten Daten aus (etwa viele Beiträge
            in wenigen Minuten oder derselbe Text immer wieder) und kann die
            jüngsten Beiträge, Kommentare, Likes und Follows eines Kontos
            gesammelt prüfen; von Direktnachrichten sieht es dabei nur die
            Anzahl, nie den Inhalt. Wir erheben dafür keine zusätzlichen Daten
            wie IP-Adressen. Jede solche Prüfung wird mit Grund, Person und
            Ergebnis protokolliert und nach einem Jahr gelöscht (Art. 6 Abs. 1
            lit. f DSGVO). Antworten auf unsere E-Mails erreichen unser
            Postfach unter kontakt@klarsocial.eu.
          </p>
        </section>

        <section className="mb-6">
          <h2 className="mb-2 font-semibold">3. Inhalte (Fotos, Beiträge, Nachrichten)</h2>
          <p>
            Hochgeladene Fotos werden serverseitig verarbeitet: Wir entfernen
            automatisch alle EXIF-Metadaten (u. a. Standortdaten, Geräteinfo,
            Aufnahmezeitpunkt), bevor das Bild gespeichert wird. Beiträge, 
            Kommentare und Direktnachrichten werden auf unseren Servern gespeichert, 
            um die Kernfunktion des Dienstes bereitzustellen und zuzustellen 
            (Art. 6 Abs. 1 lit. b DSGVO). Bitte beachte, dass Direktnachrichten 
            in der aktuellen Entwicklungsphase serverseitig noch nicht 
            Ende-zu-Ende-verschlüsselt (E2EE) sind. Die Plattform und die Datenbank 
            sind jedoch durch strenge Zugangskontrollen abgesichert.
          </p>
          <p className="mt-2">
            <strong>Feedback:</strong> Wenn du uns über das Feedback-Formular
            einen Fehler oder eine Idee schickst, speichern wir deine Nachricht,
            die gewählte Kategorie und die Verknüpfung mit deinem Konto. Nur
            wenn du die Option „Include technical details“ aktiviert lässt, speichern
            wir außerdem die Seite, von der du gekommen bist, deine
            Bildschirmgröße und die Kennung deines Browsers — das Formular
            zeigt dir diese Angaben vor dem Absenden an. Hängst du
            Screenshots an (höchstens drei), speichern wir sie ohne
            Metadaten wie Standort oder Gerät; nur Administrator:innen können
            sie ansehen. Da Screenshots auch Beiträge oder Nachrichten anderer
            Personen zeigen können, löschen wir sie 30 Tage nachdem wir dein
            Feedback bearbeitet haben, spätestens nach 90 Tagen und sofort,
            wenn du dein Konto löschst. Wir nutzen das
            Feedback nur, um Klar zu verbessern (Art. 6 Abs. 1 lit. f DSGVO),
            und löschen es nach einem Jahr; löschst du dein Konto vorher,
            entfällt die Verknüpfung mit dir.
          </p>
          <p className="mt-2">
            <strong>Benachrichtigungen:</strong> Benachrichtigungen (z. B. wer
            dir folgt oder deinen Beitrag kommentiert) löschen wir 90 Tage nach
            dem Lesen, ungelesene nach einem Jahr.
          </p>
          <p className="mt-2">
            <strong>Link-Vorschauen:</strong> Teilt jemand den Link zu einem
            Beitrag eines öffentlichen Kontos, zeigen Messenger und andere
            Dienste (z. B. WhatsApp, Signal, iMessage) eine Vorschau mit
            Nutzername, dem Anfang der Bildunterschrift und dem Bild. Die
            Vorschau erstellt der jeweilige Dienst und speichert sie in eigener
            Verantwortung; wird der Beitrag später gelöscht oder das Konto
            privat, liefern wir keine Vorschau mehr aus, bereits gespeicherte
            Vorschauen in Chats können wir aber nicht entfernen. Für private
            Konten, ausgeblendete oder mit Warnhinweis versehene Beiträge gibt
            es keine Bildvorschau.
          </p>
        </section>

        <section className="mb-6">
          <h2 className="mb-2 font-semibold">4. Hosting und Auftragsverarbeiter</h2>
          <p className="mb-2">
            Wir setzen folgende Dienstleister ein, mit denen jeweils ein
            Auftragsverarbeitungsvertrag (Art. 28 DSGVO) besteht:
          </p>
          <ul className="list-disc pl-5 space-y-2">
            <li>
              <strong>Bunny.net</strong> (Hosting der Anwendung, CDN, Datenbank und
              Speicherung von Bild-Dateien). Die Speicherung der Bild-Dateien
              erfolgt in einem deutschen Rechenzentrum. Die Ausführung der
              Anwendung selbst (Rechenleistung) wird dagegen automatisiert auf
              mehrere Rechenzentren verteilt, um kurze Ladezeiten für
              Nutzer:innen an unterschiedlichen Standorten zu ermöglichen —
              aktuell handelt es sich dabei ausschließlich um Standorte
              innerhalb der EU (u. a. Deutschland/Frankfurt als dauerhaft
              aktiver Standort, sowie bedarfsabhängig Niederlande, Österreich,
              Tschechien, Rumänien, Dänemark, Spanien, Frankreich,
              Griechenland, Kroatien, Italien, Polen und Schweden) sowie um
              das Vereinigte Königreich. Für das Vereinigte Königreich besteht
              ein gültiger Angemessenheitsbeschluss der Europäischen
              Kommission (zuletzt verlängert im Dezember 2025, gültig bis
              Dezember 2031). Es findet daher keine Drittlandübermittlung im
              Sinne von Art. 44 ff. DSGVO statt, für die zusätzliche Garantien
              (z. B. Standardvertragsklauseln) erforderlich wären.
            </li>
            <li>
              <strong>Scaleway</strong> (Versand von Transaktions-E-Mails, z.B.
              Registrierungsbestätigung und Passwort-Reset, über den Dienst
              „Transactional Email“). Scaleway ist ein französisches
              Unternehmen mit Sitz in der EU; der Versand erfolgt über die
              Region Paris (fr-par). Da es sich um einen EU-Anbieter handelt,
              ist keine Drittlandübermittlung im Sinne von Art. 44 ff. DSGVO
              involviert.
            </li>
            <li>
              <strong>Upstash</strong> (kurzzeitige Weiterleitung von
              Echtzeit-Benachrichtigungen, z. B. bei einem Like, Kommentar
              oder neuen Follower, damit diese sofort und über mehrere
              Server hinweg zugestellt werden können — siehe auch unsere{" "}
              <a href="/transparenz" className="underline">Transparenzseite</a>
              {" "}für eine ausführlichere Erklärung). Dabei werden
              ausschließlich die interne Nutzer-ID, der Benutzername, der
              Anzeigename und die Profilbild-URL der jeweils auslösenden
              Person übertragen, jedoch nicht dauerhaft bei Upstash
              gespeichert. E-Mail-Adressen werden nicht übertragen. Upstash
              ist ein US-amerikanisches Unternehmen; die Übermittlung erfolgt
              auf Grundlage von EU-Standardvertragsklauseln (Art. 46 DSGVO)
              bzw. des EU-U.S. Data Privacy Framework.
            </li>
          </ul>
          <p className="mt-2">
            Zur Überwachung unserer nächtlichen Datenbank-Sicherungen und der
            Erreichbarkeit von Klar nutzen wir außerdem{" "}
            <strong>Healthchecks.io</strong> (Anbieter mit Sitz in Lettland,
            EU). Nach jeder Sicherung und alle paar Minuten während des
            Betriebs senden unsere Server dorthin lediglich eine inhaltsleere
            Erfolgs- oder Fehlermeldung, damit wir bei einem Ausfall sofort
            benachrichtigt werden. Dabei wird nur die
            IP-Adresse unserer Server übermittelt — Daten von Nutzer:innen
            werden nicht übertragen.
          </p>
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
          <p className="mt-2">
            Zu jeder Anmeldung speichern wir die Refresh-Token nur als
            Hashwert und bis zu ihrem Ablauf nach 30 Tagen, auch bereits
            verwendete: Taucht ein verwendetes Token erneut auf, hat es
            vermutlich jemand kopiert, und wir beenden diese Anmeldung (Art. 6
            Abs. 1 lit. f DSGVO, Sicherheit deines Kontos). Änderst du dein
            Passwort oder sperren wir dein Konto wegen Verdachts auf fremden
            Zugriff, enden alle Anmeldungen sofort; dazu speichern wir den
            Zeitpunkt.
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
          <h2 className="mb-2 font-semibold">7. Meldungen und Moderation</h2>
          <p>
            Wenn du einen Beitrag, einen Kommentar, eine Direktnachricht oder
            ein Profil über die Melden-Funktion meldest, verarbeiten wir deine
            Nutzer-ID als meldende Person, den gemeldeten Inhalt bzw. das
            gemeldete Profil, den von dir ausgewählten Meldegrund sowie eine
            optionale Freitext-Beschreibung. Bei der Bearbeitung einer Meldung durch
            unser Team werden zusätzlich eine interne Notiz sowie die
            Nutzer-ID der bearbeitenden Person gespeichert. Rechtsgrundlage
            ist die Erfüllung unserer rechtlichen Pflichten als
            Hosting-Anbieter (Art. 6 Abs. 1 lit. c DSGVO i. V. m. Art. 16 des
            Digital Services Act) sowie unser berechtigtes Interesse an einer
            sicheren Plattform (Art. 6 Abs. 1 lit. f DSGVO). Bestimmte
            Meldegründe (insbesondere Darstellung sexuellen Missbrauchs von
            Minderjährigen) führen automatisiert zu einer sofortigen
            Ausblendung des gemeldeten Inhalts; andere Meldegründe können
            zunächst zu einer Kennzeichnung mit Warnhinweis führen, bis eine
            manuelle Prüfung erfolgt ist. Das geschieht nur bei Meldungen von
            Konten mit bestätigter E-Mail-Adresse, die mindestens einen Tag
            alt sind und deren Meldungen dieser Art in den letzten 30 Tagen
            nicht wiederholt unbegründet waren; dazu werten wir die eigenen
            bisherigen Meldungen des Kontos aus. Pro Tag sind höchstens 30
            Meldungen möglich. So verhindern wir, dass jemand mit neu
            angelegten Konten fremde Inhalte ausblendet (Art. 6 Abs. 1 lit. f
            DSGVO, Art. 23 DSA). Stellen wir keinen Verstoß fest, kannst du
            einmal um erneute Prüfung bitten; wir speichern dazu deine
            Begründung. Meldungen löschen wir sechs Monate nach unserer
            Entscheidung, es sei denn, eine noch gültige Punktevergabe, eine
            Beweissicherung oder ein offener Widerspruch beruht darauf.
            Löschst du dein Konto, bleiben von dir abgegebene Meldungen bis
            dahin bestehen, sind aber nicht mehr mit dir verknüpft.
          </p>
          <p className="mt-2">
            <strong>Meldungen über das Formular:</strong> Rechtswidrige Inhalte
            kann jede Person, auch ohne Klar-Konto, über das Formular
            „Rechtswidrige Inhalte melden“ melden. Dabei übermittelt sie uns den
            Link zum Inhalt, den Meldegrund, eine Begründung sowie Name und
            E-Mail-Adresse; bei Darstellungen sexuellen Missbrauchs von Kindern
            sind Name und E-Mail-Adresse freiwillig. Wir nutzen diese Angaben,
            um die Meldung zu prüfen und über Eingang und Ergebnis zu
            informieren (Art. 6 Abs. 1 lit. c DSGVO i. V. m. Art. 16 DSA). Den
            Stand kann die meldende Person über einen persönlichen Link
            einsehen; wir speichern davon nur einen nicht rückrechenbaren
            Hashwert. Wer betroffen ist, erfährt nicht, wer gemeldet hat. Die
            Angaben löschen wir zusammen mit der Meldung.
          </p>
          <p className="mt-2">
            <strong>Begründung von Entscheidungen:</strong> Wird ein Inhalt von
            dir entfernt, ausgeblendet oder mit einem Warnhinweis versehen,
            speichern wir die Entscheidung mit Begründung, Rechtsgrundlage,
            einem kurzen Auszug des betroffenen Inhalts, dem Zeitpunkt und der
            entscheidenden Person und teilen sie dir in der App und per E-Mail
            mit (Art. 17 Digital Services Act). Dort kannst du innerhalb von
            sechs Monaten widersprechen; dein Widerspruch und unsere Antwort
            werden ebenfalls gespeichert. Wer etwas meldet, erfährt, ob die
            Meldung zu einer Maßnahme geführt hat — nicht aber, wer betroffen
            ist. Rechtsgrundlage ist Art. 6 Abs. 1 lit. c DSGVO i. V. m. Art. 16
            und 17 DSA. Entfernt unser Team einen Beitrag, einen Kommentar
            oder Angaben aus deinem Profil, sind sie sofort für alle
            unsichtbar; wir bewahren sie aber bis zum Ende der sechsmonatigen
            Widerspruchsfrist auf, um sie wiederherzustellen, falls dein
            Widerspruch Erfolg hat, und löschen sie danach endgültig
            (Art. 6 Abs. 1 lit. c DSGVO i. V. m. Art. 20 Abs. 4 DSA).
            Darstellungen sexuellen Missbrauchs von Minderjährigen löschen wir,
            sobald die Beweissicherung abgeschlossen ist. Entscheidungen
            löschen wir drei Jahre nach der Entscheidung, ihrer Aufhebung oder
            der Antwort auf einen Widerspruch (regelmäßige Verjährungsfrist,
            § 195 BGB), solange keine gültige Punktevergabe oder laufende
            Sperrung darauf beruht; löschst du dein Konto vorher, entfallen
            der Inhaltsauszug, die entfernten Profilangaben und die
            Verknüpfung mit dir.
          </p>
          <p className="mt-2">
            <strong>Punkte und Kontomaßnahmen:</strong> Entfernt unser Team
            einen Inhalt von dir wegen eines Verstoßes, vermerken wir je nach
            Schwere Punkte an deinem Konto (Art. 6 Abs. 1 lit. f DSGVO; unser
            berechtigtes Interesse ist eine sichere Plattform und eine
            einheitliche, nachvollziehbare Moderation). Die Punkte verfallen
            je nach Schwere nach 90 Tagen, 180 Tagen oder einem Jahr und
            werden dann gelöscht; nur bei schwersten Verstößen verfallen sie
            nicht. Die Summe dient unserem Team als Anhaltspunkt für
            Verwarnungen und Sperrungen — entschieden wird immer von einem
            Menschen, nie automatisch (Art. 22 DSGVO). Deinen Punktestand und
            die zugrunde liegenden Entscheidungen siehst du unter
            „Moderation“ in den Einstellungen; sie sind auch in deinem
            Datenexport enthalten. Damit unser Team später nachvollziehen
            kann, worum es ging, speichern wir zu jedem Punkteeintrag den
            entfernten Text mit Zeitpunkten, bei Kommentaren den Beitrag und
            den Kommentar, auf den du geantwortet hast, sowie Grund,
            Beschreibung und Zeitpunkt der Meldungen (nicht, wer gemeldet
            hat); Bilder werden dafür nicht kopiert. Jeder Abruf durch unser
            Team wird protokolliert, das Protokoll nach einem Jahr gelöscht.
            Diese Kopie wird zusammen mit den Punkten gelöscht, also wenn sie
            verfallen, dein Widerspruch Erfolg hat oder du dein Konto löschst.
            Ein dauerhaft gesperrtes Konto löschen wir sechs Monate nach der
            Sperrung, sobald kein Widerspruch mehr offen ist, nach einer
            Erinnerung per E-Mail zwei Wochen vorher; die Entscheidung selbst
            bleibt ohne Verknüpfung mit dir als Nachweis bestehen.
          </p>
          <p className="mt-2">
            <strong>Meldungen von Rechteverletzungen:</strong> Wer über das
            Formular „Rechteverletzung melden“ geltend macht, dass ein Beitrag
            ein Urheber-, Marken- oder anderes Recht verletzt — auch ohne
            Klar-Konto —, übermittelt uns Name, E-Mail-Adresse, optional
            Organisation und vertretene Person, den Link zum Beitrag, die
            Beschreibung des Werks und die Grundlage der Rechte. Wir nutzen
            diese Angaben, um die Meldung zu prüfen, Rückfragen zu stellen und
            über das Ergebnis zu informieren (Art. 6 Abs. 1 lit. c DSGVO i. V. m.
            Art. 16 DSA). Den Stand kann die meldende Person über einen
            persönlichen Link einsehen; wir speichern davon nur einen
            nicht rückrechenbaren Hashwert. Wird die Meldung angenommen, wird
            der Beitrag ausgeblendet, und die veröffentlichende Person erfährt,
            um welches Werk es geht — nicht aber, wer gemeldet hat. Meldungen
            werden drei Jahre nach unserer Entscheidung gelöscht.
          </p>
          <p className="mt-2">
            <strong>Beweissicherung:</strong> Wird ein Beitrag, Kommentar oder
            Profil wegen eines möglicherweise strafbaren Inhalts gemeldet
            (etwa Darstellung sexuellen Missbrauchs von Minderjährigen,
            Gewalt, Hass oder Belästigung), sichern wir sofort eine Kopie des
            gemeldeten Stands und, solange die Meldung offen ist, jeder
            späteren Änderung daran. Die Kopie bleibt auch erhalten, wenn der
            Inhalt gelöscht wird — durch unsere Moderation, durch die
            verfassende Person selbst oder mit ihrem Konto. Gesichert werden
            nur der gemeldete Inhalt selbst einschließlich Bildern, die
            Kontodaten der verfassenden Person (Nutzer-ID, Nutzername,
            Anzeigename, E-Mail-Adresse, Registrierungsdatum) und der
            Zusammenhang (z. B. der kommentierte Beitrag) — keine weiteren
            Kommentare oder Likes. Bei anderen
            Meldegründen (z. B. Spam) wird nichts kopiert. Eine gemeldete
            Direktnachricht sichern wir unabhängig vom Grund, zusammen mit den
            zehn Nachrichten davor (von beiden Seiten) als Zusammenhang; unser
            Team kann sie dann lesen. Das ist nötig, weil die verfassende
            Person eine Nachricht — auch mit ihrem Konto — für beide Seiten
            löschen kann und die Meldung sonst ins Leere liefe. Weitere
            Nachrichten des Gesprächs sehen wir nicht. Die Kopie liegt in einem
            gesonderten, nicht öffentlich erreichbaren Speicher bei Bunny.net
            in Deutschland, Bilder darin verschlüsselt; nur unser Moderationsteam hat Zugriff, und jeder
            Zugriff wird mit Person, Zeitpunkt und Grund protokolliert.
            Rechtsgrundlage ist Art. 6 Abs. 1 lit. c und f DSGVO; das Recht
            auf Löschung ist insoweit nach Art. 17 Abs. 3 lit. e DSGVO
            eingeschränkt. Erweist sich die Meldung als unbegründet oder hat
            ein Widerspruch gegen die Entfernung Erfolg, löschen wir die Kopie
            umgehend, andernfalls sechs Monate nach unserer Entscheidung — es sei denn, sie wird für ein laufendes Verfahren
            oder auf Anforderung einer Behörde länger benötigt. Diese Kopien
            sind nicht Teil des Datenexports.
          </p>
        </section>

        <section className="mb-6">
          <h2 className="mb-2 font-semibold">8. Deine Rechte</h2>
          <p>
            Du hast das Recht auf Auskunft (Art. 15 DSGVO), Berichtigung (Art.
            16), Löschung (Art. 17), Einschränkung der Verarbeitung (Art. 18),
            Datenübertragbarkeit (Art. 20) sowie Widerspruch (Art. 21) gegen
            die Verarbeitung deiner Daten. Für das Auskunftsrecht und die
            Datenübertragbarkeit steht dir in den Einstellungen unter{" "}
            <strong>„Download your data“</strong> ein direkter Selbstbedienungs-Export
            zur Verfügung, der dir alle gespeicherten Daten als ZIP-Archiv
            bereitstellt (Daten als JSON-Datei, dazu deine hochgeladenen Bilder). Für alle anderen Anliegen wende dich an{" "}
            <a href="mailto:kontakt@klarsocial.eu" className="underline">
              kontakt@klarsocial.eu
            </a>
            . Außerdem steht dir ein Beschwerderecht bei einer
            Datenschutz-Aufsichtsbehörde zu.
          </p>
        </section>

        <section className="mb-6">
          <h2 className="mb-2 font-semibold">9. Löschung deines Kontos</h2>
          <p>
            Du kannst dein Konto jederzeit in den Einstellungen löschen; zum
            Schutz vor versehentlichem oder fremdem Löschen fragen wir dabei
            nach deinem Passwort. Dein Profil, deine Beiträge, Kommentare und
            Likes werden dabei
            unmittelbar mit dem Klick auf den dazugehörigen Button aus unserer
            Datenbank entfernt — <strong>nicht</strong>, wie bei anderen
            Plattformen üblich, erst nach einer Wartefrist. Einzige Ausnahme
            sind Inhalte, die wegen eines möglicherweise strafbaren Inhalts
            gemeldet wurden: Sie werden zwar ebenfalls sofort entfernt, die
            bei der Meldung gesicherte Kopie bleibt aber als Beweismittel
            erhalten (siehe Abschnitt 7).
          </p>
          <p className="mt-2">
            <strong>Direktnachrichten</strong>, die du anderen geschickt hast,
            werden ebenfalls sofort gelöscht — auch aus dem Chatverlauf deiner
            Gesprächspartner:innen. Dort bleiben nur deren eigene Nachrichten
            stehen; dein Name wird durch „Deleted User“ ersetzt, dein Profil
            ist nicht mehr verlinkt und ein Hinweis zeigt an, dass das Konto
            gelöscht wurde. Haben beide Seiten ihr Konto gelöscht, wird die
            Unterhaltung vollständig entfernt.
          </p>
          <p className="mt-2">
            <strong>Sicherungskopien:</strong> Um Datenverlust bei technischen
            Ausfällen zu verhindern, erstellen wir jede Nacht eine
            Sicherungskopie der Datenbank, gespeichert bei Bunny.net in einem
            deutschen Rechenzentrum. Jede Sicherung wird nach 14 Tagen
            automatisch gelöscht. Gelöschte Daten können daher noch bis zu 14
            Tage in diesen Sicherungen enthalten sein; sie werden
            ausschließlich zur Wiederherstellung nach einem Ausfall verwendet.
          </p>
        </section>

        <p className="text-xs text-muted-foreground">
          Stand: 29.09.2026
        </p>
      </div>
    </div>
  );
}
