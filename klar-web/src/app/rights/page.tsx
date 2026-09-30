"use client";

import { useState } from "react";
import Link from "next/link";
import { CheckCircle2 } from "lucide-react";
import { rightsApi, type RightsClaimType } from "@/lib/api";
import { Button } from "@/components/ui/button";

// The public rights-claim form (DSA Art. 16): for rightsholders, with or
// without a Klar account. In German like the legal pages, since it's a
// formal notice. ⚖️ Wording pending legal review.

const inputClass =
  "w-full rounded-md border border-input bg-transparent px-3 py-2 text-sm outline-none placeholder:text-muted-foreground focus:border-ring focus:ring-1 focus:ring-ring";

const TYPES: { value: RightsClaimType; label: string }[] = [
  { value: "copyright", label: "Urheberrecht (Foto, Text, Grafik, …)" },
  { value: "trademark", label: "Marke" },
  { value: "other", label: "Anderes Recht (z. B. Recht am eigenen Bild)" },
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

export default function RightsClaimPage() {
  const [claimType, setClaimType] = useState<RightsClaimType>("copyright");
  const [name, setName] = useState("");
  const [email, setEmail] = useState("");
  const [organization, setOrganization] = useState("");
  const [forSomeoneElse, setForSomeoneElse] = useState(false);
  const [representedParty, setRepresentedParty] = useState("");
  const [contentUrl, setContentUrl] = useState("");
  const [work, setWork] = useState("");
  const [basis, setBasis] = useState("");
  const [originalUrl, setOriginalUrl] = useState("");
  const [goodFaith, setGoodFaith] = useState(false);
  const [website, setWebsite] = useState("");

  const [sending, setSending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [created, setCreated] = useState<{ id: string; token: string } | null>(null);

  const complete =
    name.trim() && email.trim() && contentUrl.trim() && work.trim() && basis.trim() && goodFaith &&
    (!forSomeoneElse || representedParty.trim());

  const submit = async () => {
    setSending(true);
    setError(null);
    try {
      setCreated(await rightsApi.create({
        claim_type: claimType,
        claimant_name: name,
        claimant_email: email,
        claimant_organization: organization || null,
        represented_party: forSomeoneElse ? representedParty : null,
        content_url: contentUrl,
        work_description: work,
        ownership_basis: basis,
        original_url: originalUrl || null,
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
    <div className="min-h-screen bg-background">
      <main className="mx-auto max-w-xl px-4 py-8">
        <h1 className="mb-2 text-2xl font-bold">Rechteverletzung melden</h1>

        {created ? (
          <div className="rounded-xl border border-border p-6">
            <CheckCircle2 size={32} className="mb-3 text-muted-foreground" />
            <p className="font-semibold">Deine Meldung ist eingegangen.</p>
            <p className="mt-2 text-sm text-muted-foreground">
              Wir haben dir eine Bestätigung an {email} geschickt. Den Stand deiner Meldung und
              mögliche Rückfragen findest du unter deinem persönlichen Link — bewahre ihn auf und gib ihn nicht weiter.
            </p>
            <Link href={`/rights/status/${created.id}#${created.token}`} className="mt-4 inline-block text-sm underline">
              Zum Stand deiner Meldung
            </Link>
          </div>
        ) : (
          <form onSubmit={(e) => { e.preventDefault(); submit(); }} className="space-y-5">
            <p className="text-sm text-muted-foreground">
              Verletzt ein Beitrag auf Klar dein Urheberrecht oder ein anderes Recht, kannst du ihn hier melden — auch
              ohne Klar-Konto. Wir prüfen jede Meldung durch einen Menschen. Für andere Verstöße nutze bitte die
              Melden-Funktion in der App. Wie wir deine Angaben verarbeiten, steht in der{" "}
              <Link href="/datenschutz" className="underline">Datenschutzerklärung</Link>.
            </p>

            <fieldset className="space-y-2">
              <legend className="mb-1 text-sm font-medium">Um welches Recht geht es?</legend>
              {TYPES.map((t) => (
                <label key={t.value} className="flex items-center gap-2 text-sm">
                  <input type="radio" name="claim_type" checked={claimType === t.value} onChange={() => setClaimType(t.value)} />
                  {t.label}
                </label>
              ))}
            </fieldset>

            <Field label="Link zum Beitrag auf Klar" hint="z. B. https://www.klarsocial.eu/posts/…">
              <input value={contentUrl} onChange={(e) => setContentUrl(e.target.value)} maxLength={500} className={inputClass} />
            </Field>

            <Field label="Welches Werk wird verletzt?" hint="Beschreibe das Foto, den Text oder die Marke so genau wie möglich.">
              <textarea value={work} onChange={(e) => setWork(e.target.value)} maxLength={4000} rows={3} className={inputClass} />
            </Field>

            <Field label="Warum stehen dir die Rechte zu?" hint="z. B. „Ich habe das Foto am 3. Mai 2026 selbst aufgenommen“ oder „Wir halten die ausschließlichen Nutzungsrechte“.">
              <textarea value={basis} onChange={(e) => setBasis(e.target.value)} maxLength={4000} rows={3} className={inputClass} />
            </Field>

            <Field label="Link zum Original (optional)" hint="Wo das Werk von dir veröffentlicht ist, falls online.">
              <input value={originalUrl} onChange={(e) => setOriginalUrl(e.target.value)} maxLength={500} className={inputClass} />
            </Field>

            <Field label="Dein Name">
              <input value={name} onChange={(e) => setName(e.target.value)} maxLength={200} autoComplete="name" className={inputClass} />
            </Field>
            <Field label="Deine E-Mail-Adresse" hint="Hierhin schicken wir die Bestätigung und das Ergebnis.">
              <input type="email" value={email} onChange={(e) => setEmail(e.target.value)} maxLength={255} autoComplete="email" className={inputClass} />
            </Field>
            <Field label="Unternehmen oder Organisation (optional)">
              <input value={organization} onChange={(e) => setOrganization(e.target.value)} maxLength={200} autoComplete="organization" className={inputClass} />
            </Field>

            <label className="flex items-center gap-2 text-sm">
              <input type="checkbox" checked={forSomeoneElse} onChange={(e) => setForSomeoneElse(e.target.checked)} />
              Ich melde im Auftrag der Person oder Firma, der die Rechte zustehen
            </label>
            {forSomeoneElse && (
              <Field label="Für wen meldest du?">
                <input value={representedParty} onChange={(e) => setRepresentedParty(e.target.value)} maxLength={200} className={inputClass} />
              </Field>
            )}

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
                Ich bin in gutem Glauben davon überzeugt, dass die Angaben in dieser Meldung richtig und vollständig
                sind und dass die Nutzung des Werks nicht erlaubt ist.
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
