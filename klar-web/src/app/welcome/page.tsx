"use client";

import { useState } from "react";

/**
 * The gate itself -- everything else on the site redirects here until
 * the passcode is entered (see middleware.ts). Legal-page links aren't
 * duplicated here since the global Footer (rendered on every page,
 * including this one) already links to them.
 */
export default function WelcomePage() {
  const [passcode, setPasscode] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!passcode.trim() || loading) return;

    setLoading(true);
    setError(null);

    try {
      const res = await fetch("/api/site-access", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ passcode: passcode.trim() }),
      });

      if (!res.ok) {
        const data = await res.json().catch(() => null);
        throw new Error(data?.error ?? "Falscher Zugangscode");
      }

      // Full reload (not router.push) so the middleware re-evaluates
      // cleanly against the freshly-set cookie on the very next request.
      window.location.href = "/";
    } catch (err) {
      setError(err instanceof Error ? err.message : "Etwas ist schiefgelaufen");
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="flex min-h-[calc(100dvh-3rem)] flex-col items-center justify-center bg-background px-4 text-center">
      <div className="w-full max-w-sm">
        <h1 className="mb-3 text-3xl font-bold tracking-tight">Klar</h1>
        <p className="mb-8 text-sm leading-relaxed text-muted-foreground">
          Hier entsteht Klar — eine private, werbefreie Social-Media-App
          ohne Algorithmus: Du siehst Beiträge chronologisch, ganz ohne
          Sortierung nach Interaktionen, und behältst die Kontrolle
          darüber, wer deine Inhalte sehen kann. Wir sind aktuell im
          Aufbau und öffnen bald für alle.
        </p>

        <form onSubmit={handleSubmit} className="space-y-3">
          <input
            type="password"
            value={passcode}
            onChange={(e) => setPasscode(e.target.value)}
            placeholder="Zugangscode"
            autoFocus
            disabled={loading}
            className="w-full rounded-md border border-input bg-transparent px-3 py-2 text-center text-sm outline-none placeholder:text-muted-foreground focus:border-ring focus:ring-1 focus:ring-ring disabled:opacity-50"
          />
          {error && <p className="text-sm text-destructive">{error}</p>}
          <button
            type="submit"
            disabled={loading || !passcode.trim()}
            className="w-full rounded-md bg-primary px-3 py-2 text-sm font-medium text-primary-foreground transition-opacity disabled:opacity-50"
          >
            {loading ? "Prüfe…" : "Weiter"}
          </button>
        </form>
      </div>
    </div>
  );
}
