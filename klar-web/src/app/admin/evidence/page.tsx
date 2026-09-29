"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { Archive, ChevronRight } from "lucide-react";
import { useAuth } from "@/lib/auth-context";
import { adminEvidenceApi, type EvidenceSummary } from "@/lib/api";
import { SmartBackButton } from "@/components/SmartBackButton";
import { REASON_LABELS, TRIGGER_LABELS } from "@/lib/moderation";
import EvidenceStatus from "@/components/EvidenceStatus";

export default function AdminEvidencePage() {
  const { user, loading: authLoading } = useAuth();
  const router = useRouter();

  const [records, setRecords] = useState<EvidenceSummary[]>([]);
  const [includePurged, setIncludePurged] = useState(false);
  // The filter the loaded list belongs to; loading is derived from it so
  // toggling the filter shows the spinner without a setState in the effect.
  const [loadedFor, setLoadedFor] = useState<boolean | null>(null);
  const loading = loadedFor !== includePurged;
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!authLoading && !user) router.push("/login");
  }, [user, authLoading, router]);

  useEffect(() => {
    if (!user) return;
    let cancelled = false;
    adminEvidenceApi.list(includePurged)
      .then((data) => { if (!cancelled) { setRecords(data); setError(null); } })
      .catch((err) => { if (!cancelled) setError(err instanceof Error ? err.message : "Failed to load evidence"); })
      .finally(() => { if (!cancelled) setLoadedFor(includePurged); });
    return () => { cancelled = true; };
  }, [user, includePurged]);

  if (authLoading || !user) return null;

  return (
    <div className="min-h-screen bg-background">
      <header className="sticky top-0 z-10 flex h-14 items-center gap-3 border-b border-border bg-background/80 px-4 backdrop-blur">
        <SmartBackButton aria-label="Back" />
        <span className="flex-1 font-semibold">Evidence</span>
        <label className="flex items-center gap-2 text-sm text-muted-foreground">
          <input
            type="checkbox"
            checked={includePurged}
            onChange={(e) => setIncludePurged(e.target.checked)}
          />
          Show purged
        </label>
      </header>

      <main className="mx-auto max-w-2xl px-4 py-4">
        <p className="mb-4 text-sm text-muted-foreground">
          Copies of reported content that was deleted while a report for a likely-illegal reason was pending.
          Opening a record or file asks for a reason and is logged.
        </p>

        {error && (
          <div className="mb-4 rounded-md bg-destructive/10 px-3 py-2 text-sm text-destructive">{error}</div>
        )}

        {loading && (
          <div className="py-16 text-center text-sm text-muted-foreground animate-pulse">Loading…</div>
        )}

        {!loading && !error && records.length === 0 && (
          <div className="py-16 text-center">
            <Archive size={32} className="mx-auto mb-3 text-muted-foreground" />
            <p className="text-sm text-muted-foreground">No preserved evidence</p>
          </div>
        )}

        {!loading && (
          <div className="space-y-2">
            {records.map((record) => (
              <Link
                key={record.id}
                href={`/admin/evidence/${record.id}`}
                className="flex items-center gap-3 rounded-xl border border-border p-3 hover:bg-muted/40"
              >
                <div className="min-w-0 flex-1">
                  <div className="mb-1 flex flex-wrap items-center gap-2">
                    <span className="text-sm font-medium capitalize">{record.target_type}</span>
                    <EvidenceStatus record={record} />
                  </div>
                  <p className="truncate text-sm">
                    {record.reasons.map((r) => REASON_LABELS[r] ?? r).join(", ") || "—"}
                  </p>
                  <p className="text-xs text-muted-foreground">
                    {TRIGGER_LABELS[record.trigger]} · {new Date(record.created_at).toLocaleString()}
                    {record.file_count > 0 && ` · ${record.file_count} file${record.file_count === 1 ? "" : "s"}`}
                  </p>
                </div>
                <ChevronRight size={16} className="shrink-0 text-muted-foreground" />
              </Link>
            ))}
          </div>
        )}
      </main>
    </div>
  );
}
