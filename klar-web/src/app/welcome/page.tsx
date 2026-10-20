"use client";

import { useState } from "react";
import { useTranslations } from "next-intl";

/**
 * The gate itself -- everything else on the site redirects here until
 * the passcode is entered (see proxy.ts). Legal-page links aren't
 * duplicated here since the global Footer (rendered on every page,
 * including this one) already links to them.
 */
export default function WelcomePage() {
  const t = useTranslations("welcome");
  const tc = useTranslations("common");
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

      // The route's own messages are English; the status says enough.
      if (res.status === 401) {
        setError(t("wrongPasscode"));
        return;
      }
      if (res.status === 429) {
        setError(t("tooManyAttempts"));
        return;
      }
      if (!res.ok) {
        setError(tc("somethingWentWrong"));
        return;
      }

      // Full reload (not router.push) so the proxy re-evaluates cleanly
      // against the freshly-set cookie on the very next request.
      window.location.href = "/";
    } catch {
      setError(tc("somethingWentWrong"));
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="flex flex-1 flex-col items-center justify-center bg-background px-4 text-center">
      <div className="w-full max-w-sm">
        <h1 className="mb-3 text-3xl font-bold tracking-tight">Klar</h1>
        <p className="mb-8 text-sm leading-relaxed text-muted-foreground">{t("intro")}</p>

        <form onSubmit={handleSubmit} className="space-y-3">
          <input
            type="password"
            value={passcode}
            onChange={(e) => setPasscode(e.target.value)}
            placeholder={t("passcode")}
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
            {loading ? t("checking") : t("continue")}
          </button>
        </form>
      </div>
    </div>
  );
}
