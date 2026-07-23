import Link from "next/link";
import { ArrowLeft } from "lucide-react";

export default function ImpressumPage() {
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
          <span className="font-semibold">Impressum</span>
        </div>
      </header>

      <div className="mx-auto max-w-2xl px-4 py-10 text-sm leading-relaxed">
        <section className="mb-6">
          <h2 className="mb-2 font-semibold">Angaben gemäß § 5 TMG</h2>
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
          <h2 className="mb-2 font-semibold">
            Verantwortlich für den Inhalt nach § 18 Abs. 2 MStV
          </h2>
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
    </div>
  );
}
