"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import { usePathname } from "next/navigation";
import { FileText } from "lucide-react";
import { useAuth } from "@/lib/auth-context";
import { legalUpdatesApi, type LegalUpdate } from "@/lib/api";
import { Button } from "@/components/ui/button";

// Pages the notice stays out of, so people can read the new version, export
// their data or delete their account before (or instead of) accepting.
const OPEN_PAGES = ["/nutzungsbedingungen", "/datenschutz", "/settings", "/settings/account", "/impressum"];

function documentsLabel(u: LegalUpdate) {
  const terms = u.documents.includes("terms");
  const privacy = u.documents.includes("privacy");
  if (terms && privacy) return "unsere Nutzungsbedingungen und unsere Datenschutzerklärung";
  return terms ? "unsere Nutzungsbedingungen" : "unsere Datenschutzerklärung";
}

// Shown to signed-in users once per notice about changed Terms or privacy
// policy (German, like the legal pages). A Terms change can only be
// accepted, not dismissed; the acceptance is recorded as proof.
export default function LegalUpdateNotice() {
  const { user } = useAuth();
  const pathname = usePathname();
  const [pending, setPending] = useState<LegalUpdate[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!user) return;
    let cancelled = false;
    legalUpdatesApi.pending()
      .then((updates) => { if (!cancelled) setPending(updates); })
      .catch(() => {});
    return () => { cancelled = true; };
  }, [user]);

  const update = pending[0];
  if (!user || !update || OPEN_PAGES.includes(pathname)) return null;

  const acknowledge = async () => {
    setBusy(true);
    setError(null);
    try {
      await legalUpdatesApi.acknowledge(update.id);
      setPending((prev) => prev.slice(1));
    } catch (err) {
      setError(err instanceof Error ? err.message : "Das hat nicht geklappt");
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4">
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="legal-update-title"
        className="max-h-[90dvh] w-full max-w-md overflow-y-auto rounded-xl bg-background p-5 shadow-xl"
      >
        <h2 id="legal-update-title" className="mb-2 flex items-center gap-2 font-semibold">
          <FileText size={18} /> Wir haben {documentsLabel(update)} geändert
        </h2>
        <p className="mb-1 text-xs text-muted-foreground">
          {new Date(update.published_at).toLocaleDateString("de-DE")}
        </p>
        <p className="mb-3 text-sm font-medium">Das ist neu:</p>
        <p className="mb-4 whitespace-pre-wrap rounded-md bg-muted/50 p-3 text-sm">{update.summary}</p>
        <p className="mb-4 text-sm">
          Die vollständige Fassung:{" "}
          {update.documents.includes("terms") && (
            <Link href="/nutzungsbedingungen" className="underline">Nutzungsbedingungen</Link>
          )}
          {update.documents.length > 1 && " · "}
          {update.documents.includes("privacy") && (
            <Link href="/datenschutz" className="underline">Datenschutzerklärung</Link>
          )}
        </p>
        {update.requires_acceptance && (
          <p className="mb-4 text-xs text-muted-foreground">
            Um Klar weiter zu nutzen, stimme bitte den neuen Nutzungsbedingungen zu. Bist du nicht
            einverstanden, kannst du in den{" "}
            <Link href="/settings" className="underline">Einstellungen</Link> deine Daten exportieren und dein
            Konto löschen.
          </p>
        )}
        {error && <p className="mb-3 text-sm text-destructive">{error}</p>}
        <Button className="w-full" onClick={acknowledge} disabled={busy}>
          {update.requires_acceptance ? "Zustimmen" : "Verstanden"}
        </Button>
      </div>
    </div>
  );
}
