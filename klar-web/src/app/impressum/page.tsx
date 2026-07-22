export default function ImpressumPage() {
  return (
    <div className="mx-auto max-w-2xl px-4 py-10 text-sm leading-relaxed">
      <h1 className="mb-6 text-2xl font-bold">Impressum</h1>

      <section className="mb-6">
        <h2 className="mb-2 font-semibold">Angaben gemäß § 5 TMG</h2>
        <p>
          Jan Motulla
          <br />
          Benzstr. 1
          <br />
          88250, Weingarten
          <br />
          Deutschland
        </p>
      </section>

      <section className="mb-6">
        <h2 className="mb-2 font-semibold">Kontakt</h2>
        <p>
          E-Mail: [kontakt@klarsocial.eu]
        </p>
      </section>

      <section className="mb-6">
        <h2 className="mb-2 font-semibold">
          Verantwortlich für den Inhalt nach § 18 Abs. 2 MStV
        </h2>
        <p>
          Jan Motulla
          <br />
          Benzstr. 1
          <br />
          88250, Weingarten
          <br />
          Deutschland
        </p>
      </section>

      <section className="mb-6">
        <h2 className="mb-2 font-semibold">Streitschlichtung</h2>
        <p>
          Die Europäische Kommission stellt eine Plattform zur
          Online-Streitbeilegung (OS) bereit:{" "}
          <a
            href="https://ec.europa.eu/consumers/odr/"
            target="_blank"
            rel="noopener noreferrer"
            className="underline"
          >
            https://ec.europa.eu/consumers/odr/
          </a>
          . Wir sind nicht verpflichtet und nicht bereit, an
          Streitbeilegungsverfahren vor einer Verbraucherschlichtungsstelle
          teilzunehmen.
        </p>
      </section>
    </div>
  );
}
