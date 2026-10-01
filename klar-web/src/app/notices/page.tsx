"use client";

import { useState } from "react";
import Link from "next/link";
import { CheckCircle2 } from "lucide-react";
import { noticesApi, type ReportReason } from "@/lib/api";
import { Button } from "@/components/ui/button";

// The public form for notices about illegal content (DSA Art. 16): for
// anyone, with or without a Klar account. In German like the legal pages,
// since it's a formal notice. A notice goes into the same queue as reports
// from the app; the notifier gets a status link and the outcome by email.
// ⚖️ Wording pending legal review.

const inputClass =
  "w-full rounded-md border border-input bg-transparent px-3 py-2 text-sm outline-none placeholder:text-muted-foreground focus:border-ring focus:ring-1 focus:ring-ring";

const REASONS: { value: ReportReason; label: string }[] = [
  { value: "csam", label: "Darstellung sexuellen Missbrauchs von Kindern" },
  { value: "ncii", label: "Intime Aufnahmen ohne Einwilligung" },
  { value: "terrorism", label: "Terrorismus oder Androhung schwerer Gewalt" },
  { value: "hate_speech", label: "Volksverhetzung oder Hassrede" },
  { value: "extremism", label: "Verbotene Symbole, extremistische Propaganda" },
  { value: "harassment", label: "Bedrohung, Beleidigung oder Stalking" },
  { value: "violence", label: "Gewaltdarstellung" },
  { value: "sexual_content", label: "Pornografie oder sexuelle Belästigung" },
  { value: "self_harm", label: "Anleitung oder Aufforderung zur Selbstverletzung" },
  { value: "fraud", label: "Betrug" },
  { value: "illegal_goods", label: "Handel mit illegalen Waren (Drogen, Waffen)" },
  { value: "impersonation", label: "Identitätsdiebstahl" },
  { value: "other", label: "Anderer rechtswidriger Inhalt" },
];

function Field({ label, hint, children }: { label: string; hint?: string; children: React.ReactNode }) {
  return (
    <label className="block space-y-1">
      <span className="text-sm font-medium">{label}</span>
      {hint && <span className="block text-xs text-muted-foreground">{hint}</span>}
      {children}
    </label>
  );
}

export default function NoticePage() {
  const [reason, setReason] = useState<ReportReason | null>(null);
  const [contentUrl, setContentUrl] = useState("");
  const [explanation, setExplanation] = useState("");
  const [name, setName] = useState("");
  const [email, setEmail] = useState("");
  const [goodFaith, setGoodFaith] = useState(false);
  const [website, setWebsite] = useState("");

  const [sending, setSending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [created, setCreated] = useState<{ id: string; token: string } | null>(null);

  // Child sexual abuse material can be reported anonymously (Art. 16(2)(c)).
  const anonymousAllowed = reason === "csam";
  const complete =
    reason && contentUrl.trim() && explanation.trim() && goodFaith &&
    (anonymousAllowed || (name.trim() && email.trim()));

  const submit = async () => {
    if (!reason) return;
    setSending(true);
    setError(null);
    try {
      setCreated(await noticesApi.create({
        reason,
        explanation,
        content_url: contentUrl,
        notifier_name: name.trim() || null,
        notifier_email: email.trim() || null,
        good_faith: goodFaith,
        website,
      }));
    } catch (err) {
      setError(err instanceof Error ? err.message : "Die Meldung konnte nicht gesendet werden");
    } finally {
      setSending(false);
    }
  };

  return (
    <div className="flex-1 bg-background">
      <main className="mx-auto max-w-xl px-4 py-8">
        <h1 className="mb-2 text-2xl font-bold">Rechtswidrige Inhalte melden</h1>

        {created ? (
          <div className="rounded-xl border border-border p-6">
            <CheckCircle2 size={32} className="mb-3 text-muted-foreground" />
            <p className="font-semibold">Deine Meldung ist eingegangen.</p>
            <p className="mt-2 text-sm text-muted-foreground">
              {email.trim()
                ? `Wir haben dir eine Bestätigung an ${email.trim()} geschickt und informieren dich dort über unsere Entscheidung. `
                : "Du hast keine E-Mail-Adresse angegeben; den Stand siehst du nur über den folgenden Link. "}
              Bewahre den Link auf und gib ihn nicht weiter — er ist dein Zugang zu dieser Meldung.
            </p>
            <Link href={`/notices/status/${created.id}#${created.token}`} className="mt-4 inline-block text-sm underline">
              Zum Stand deiner Meldung
            </Link>
          </div>
        ) : (
          <form onSubmit={(e) => { e.preventDefault(); submit(); }} className="space-y-5">
            <p className="text-sm text-muted-foreground">
              Hier kannst du Inhalte auf Klar melden, die du für rechtswidrig hältst — auch ohne Klar-Konto. Jede
              Meldung prüft ein Mensch. Mit einem Konto geht es schneller über „Melden“ direkt am Beitrag, Kommentar oder
              Profil. Für Urheberrechte und Marken gibt es ein{" "}
              <Link href="/rights" className="underline">eigenes Formular</Link>. Wie wir deine Angaben verarbeiten,
              steht in der <Link href="/datenschutz" className="underline">Datenschutzerklärung</Link>.
            </p>

            <fieldset className="space-y-1.5">
              <legend className="mb-1 text-sm font-medium">Worum geht es?</legend>
              {REASONS.map((r) => (
                <label key={r.value} className="flex items-center gap-2 text-sm">
                  <input type="radio" name="reason" checked={reason === r.value} onChange={() => setReason(r.value)} />
                  {r.label}
                </label>
              ))}
            </fieldset>

            {reason === "csam" && (
              <p className="rounded-md bg-amber-500/10 p-3 text-sm">
                Bitte lade den Inhalt nicht herunter, mache keine Screenshots und leite ihn nicht weiter — auch nicht, um
                ihn zu melden: Das kann selbst strafbar sein. Ein Link genügt; wir sehen das Original. Du kannst den
                Inhalt auch bei{" "}
                <a href="https://www.jugendschutz.net/verstoss-melden" target="_blank" rel="noopener noreferrer" className="underline">
                  jugendschutz.net
                </a>{" "}
                oder der Polizei melden.
              </p>
            )}

            <Field
              label="Link zum Inhalt auf Klar"
              hint="Zu einem Beitrag (…/posts/…), einem Kommentar (über „Link“ am Kommentar) oder einem Profil (…/users/…)."
            >
              <input value={contentUrl} onChange={(e) => setContentUrl(e.target.value)} maxLength={500} className={inputClass} />
            </Field>

            <Field
              label="Warum ist der Inhalt rechtswidrig?"
              hint="Beschreibe so genau wie möglich, was gegen welches Recht verstößt, z. B. welche Aussage eine Bedrohung ist."
            >
              <textarea value={explanation} onChange={(e) => setExplanation(e.target.value)} maxLength={4000} rows={4} className={inputClass} />
            </Field>

            <Field label={anonymousAllowed ? "Dein Name (freiwillig)" : "Dein Name"}>
              <input value={name} onChange={(e) => setName(e.target.value)} maxLength={200} autoComplete="name" className={inputClass} />
            </Field>
            <Field
              label={anonymousAllowed ? "Deine E-Mail-Adresse (freiwillig)" : "Deine E-Mail-Adresse"}
              hint="Hierhin schicken wir die Bestätigung und unsere Entscheidung."
            >
              <input type="email" value={email} onChange={(e) => setEmail(e.target.value)} maxLength={255} autoComplete="email" className={inputClass} />
            </Field>

            {/* Honeypot: hidden from people, filled in by bots. */}
            <input
              value={website}
              onChange={(e) => setWebsite(e.target.value)}
              name="website"
              tabIndex={-1}
              autoComplete="off"
              aria-hidden="true"
              className="absolute -left-[9999px] h-0 w-0 opacity-0"
            />

            <label className="flex items-start gap-2 text-sm">
              <input type="checkbox" checked={goodFaith} onChange={(e) => setGoodFaith(e.target.checked)} className="mt-1" />
              <span>
                Ich bin in gutem Glauben davon überzeugt, dass die Angaben in dieser Meldung richtig und vollständig sind.
              </span>
            </label>

            {error && <div className="rounded-md bg-destructive/10 px-3 py-2 text-sm text-destructive">{error}</div>}

            <Button type="submit" disabled={sending || !complete}>
              {sending ? "Wird gesendet…" : "Meldung absenden"}
            </Button>
          </form>
        )}
      </main>
    </div>
  );
}
