"use client";

import { useEffect, useState } from "react";
import { useRouter } from "next/navigation";
import { Download } from "lucide-react";
import { useAuth } from "@/lib/auth-context";
import { adminAuditApi, type AuditExportEntry } from "@/lib/api";
import { SmartBackButton } from "@/components/SmartBackButton";
import { Button } from "@/components/ui/button";

const inputClass = "h-9 rounded-md border border-border bg-background px-2 text-sm";
const REASON_MIN = 5;

// Dates as the backend reads them (UTC), for the date inputs.
const isoDay = (d: Date) => d.toISOString().slice(0, 10);

// The moderation records of a period as a ZIP of CSVs, for an authority's
// request or a transparency report (handlers/audit_export.rs). Every export
// is logged with its reason before the file comes back.
export default function AuditExportPage() {
  const { user, loading: authLoading } = useAuth();
  const router = useRouter();

  const today = new Date();
  const [from, setFrom] = useState(isoDay(new Date(Date.UTC(today.getUTCFullYear(), today.getUTCMonth() - 1, 1))));
  const [to, setTo] = useState(isoDay(today));
  const [withIdentities, setWithIdentities] = useState(false);
  const [reason, setReason] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [exports, setExports] = useState<AuditExportEntry[] | null>(null);

  useEffect(() => {
    if (!authLoading && !user) router.push("/login");
  }, [user, authLoading, router]);

  const load = () =>
    adminAuditApi.list()
      .then((list) => { setExports(list); setError(null); })
      .catch((err) => setError(err instanceof Error ? err.message : "Failed to load the exports"));

  useEffect(() => {
    if (user) load();
  }, [user]);

  if (authLoading || !user) return null;

  const valid = reason.trim().length >= REASON_MIN && from <= to && to <= isoDay(new Date());

  const download = async () => {
    setBusy(true);
    setError(null);
    try {
      await adminAuditApi.download(from, to, withIdentities, reason.trim());
      setReason("");
      setWithIdentities(false);
      await load();
    } catch (err) {
      setError(err instanceof Error ? err.message : "The export failed");
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="flex-1 bg-background">
      <header className="sticky top-0 z-10 flex h-14 items-center gap-3 border-b border-border bg-background/80 px-4 backdrop-blur">
        <SmartBackButton aria-label="Back" />
        <span className="flex-1 font-semibold">Audit export</span>
      </header>

      <main className="mx-auto max-w-2xl space-y-6 px-4 py-4">
        <p className="text-sm text-muted-foreground">
          The moderation records of a period for an authority&apos;s request or a transparency report: decisions with
          their statements, reports and notices, objections, rights claims, the evidence access log, account locks and
          reviews, and earlier exports, as CSV files with a summary and a README (German). No content and no email
          addresses. Every export is logged with its reason.
        </p>

        {error && <div className="rounded-md bg-destructive/10 px-3 py-2 text-sm text-destructive">{error}</div>}

        <section aria-label="New export" className="space-y-3 rounded-xl border border-border p-3">
          <div className="flex flex-wrap gap-3">
            <label className="flex flex-col gap-1 text-sm">
              From
              <input type="date" className={inputClass} value={from} max={to} onChange={(e) => setFrom(e.target.value)} />
            </label>
            <label className="flex flex-col gap-1 text-sm">
              To
              <input
                type="date"
                className={inputClass}
                value={to}
                min={from}
                max={isoDay(new Date())}
                onChange={(e) => setTo(e.target.value)}
              />
            </label>
          </div>
          <label className="flex flex-col gap-1 text-sm">
            Reason (who asked, and what for)
            <textarea
              className="min-h-20 rounded-md border border-border bg-background px-2 py-1 text-sm"
              value={reason}
              maxLength={1000}
              placeholder="e.g. Bundesnetzagentur, request of 1 Oct 2026, ref. 123"
              onChange={(e) => setReason(e.target.value)}
            />
          </label>
          <label className="flex items-start gap-2 text-sm">
            <input type="checkbox" className="mt-1" checked={withIdentities} onChange={(e) => setWithIdentities(e.target.checked)} />
            <span>
              With identities
              <span className="block text-xs text-muted-foreground">
                Usernames and item IDs instead of pseudonyms. Only when the request needs them; the log records it.
              </span>
            </span>
          </label>
          <Button onClick={download} disabled={!valid || busy}>
            <Download size={16} className="mr-1" /> {busy ? "Preparing…" : "Download export"}
          </Button>
        </section>

        <section aria-labelledby="earlier-exports">
          <h2 id="earlier-exports" className="mb-2 text-sm font-semibold">Earlier exports</h2>
          {exports === null && !error && (
            <p className="text-sm text-muted-foreground animate-pulse">Loading…</p>
          )}
          {exports?.length === 0 && <p className="text-sm text-muted-foreground">No exports yet.</p>}
          <ul className="space-y-2">
            {exports?.map((x) => (
              <li key={x.id} className="rounded-lg border border-border p-2 text-sm">
                <div className="flex flex-wrap items-center gap-2">
                  <span className="font-medium">{x.period_from} – {x.period_to}</span>
                  {x.with_identities && (
                    <span className="rounded bg-amber-500/15 px-1.5 py-0.5 text-xs font-semibold text-amber-600">
                      with identities
                    </span>
                  )}
                  <span className="ml-auto text-xs text-muted-foreground">{new Date(x.created_at).toLocaleString()}</span>
                </div>
                <p className="text-xs text-muted-foreground">
                  By {x.exported_by_username ? `@${x.exported_by_username}` : "a former admin"} · {x.reason}
                </p>
              </li>
            ))}
          </ul>
        </section>
      </main>
    </div>
  );
}
