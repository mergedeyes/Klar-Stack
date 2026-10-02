"use client";

import { useCallback, useEffect, useState } from "react";
import Link from "next/link";
import { Button } from "@/components/ui/button";
import { adminReportsApi, type ClosedReport, type ReportOutcome } from "@/lib/api";
import { OUTCOME_LABELS, REASON_LABELS, SOURCE_LABELS } from "@/lib/moderation";

const PAGE = 50;

// The report queue's closed reports, most recently closed first: what was
// decided, by whom and when. Dismissals leave no decision, so this is the
// only place they show. No content: the decision and the evidence record
// are linked instead.
export default function ClosedReports({ onError }: { onError: (msg: string) => void }) {
  const [outcome, setOutcome] = useState<ReportOutcome | "">("");
  const [rows, setRows] = useState<ClosedReport[]>([]);
  // The filter the loaded rows belong to; loading is derived from it.
  const [loadedFor, setLoadedFor] = useState<string | null>(null);
  const loading = loadedFor !== outcome;
  const [more, setMore] = useState(false);

  const load = useCallback((after?: ClosedReport) => {
    adminReportsApi
      .closed({ outcome: outcome || undefined, limit: PAGE, before_time: after?.reviewed_at ?? undefined, before_id: after?.id })
      .then((page) => {
        setRows((prev) => (after ? [...prev, ...page] : page));
        setMore(page.length === PAGE);
      })
      .catch((err) => onError(err instanceof Error ? err.message : "Failed to load closed reports"))
      .finally(() => setLoadedFor(outcome));
  }, [outcome, onError]);

  useEffect(() => {
    load();
  }, [load]);

  return (
    <section aria-label="Closed reports" className="space-y-3">
      <div className="flex flex-wrap items-center gap-2">
        <select
          aria-label="Outcome"
          className="h-9 rounded-md border border-border bg-background px-2 text-sm"
          value={outcome}
          onChange={(e) => setOutcome(e.target.value as ReportOutcome | "")}
        >
          <option value="">Any outcome</option>
          {(Object.keys(OUTCOME_LABELS) as ReportOutcome[]).map((o) => (
            <option key={o} value={o}>{OUTCOME_LABELS[o]}</option>
          ))}
        </select>
        <span className="text-xs text-muted-foreground">Kept six months after the decision.</span>
      </div>

      {loading && <div className="py-16 text-center text-sm text-muted-foreground animate-pulse">Loading…</div>}
      {!loading && rows.length === 0 && (
        <p className="py-16 text-center text-sm text-muted-foreground">No closed reports</p>
      )}

      {!loading && (
        <ul className="space-y-2">
          {rows.map((r) => (
            <li key={r.id} data-testid="closed-report" className="rounded-lg border border-border p-2 text-sm">
              <div className="flex flex-wrap items-center gap-2">
                <span className="font-medium">{r.outcome ? OUTCOME_LABELS[r.outcome] : r.status}</span>
                <span className="ml-auto text-xs text-muted-foreground">
                  {r.reviewed_at && new Date(r.reviewed_at).toLocaleString()}
                </span>
              </div>
              <p className="text-xs text-muted-foreground">
                {REASON_LABELS[r.reason] ?? r.reason} · {r.target_type}
                {r.target_username && (
                  <> by <Link href={`/admin/standing/${r.target_username}`} className="underline">@{r.target_username}</Link></>
                )}
                {" · "}
                {r.source === "user_report" && r.reporter_username
                  ? `reported by @${r.reporter_username}`
                  : SOURCE_LABELS[r.source]}
                {r.authority && ` (${r.authority}${r.order_reference ? `, ${r.order_reference}` : ""})`}
                {" · "}
                {r.reviewed_by_username ? `closed by @${r.reviewed_by_username}` : "closed automatically"}
                {r.recheck_requested_at && " · re-checked"}
              </p>
              {r.review_note && <p className="mt-1 text-xs">Note: {r.review_note}</p>}
              {(r.decision_id || r.evidence_id) && (
                <p className="mt-1 flex gap-3 text-xs">
                  {/* The statement page is only the affected user's; the team
                      finds the decision in the log. */}
                  {r.decision_id && <Link href="/admin/decisions" className="underline">Decision in the log</Link>}
                  {r.evidence_id && <Link href={`/admin/evidence/${r.evidence_id}`} className="underline">Evidence</Link>}
                </p>
              )}
            </li>
          ))}
        </ul>
      )}

      {!loading && more && (
        <Button variant="outline" size="sm" onClick={() => load(rows[rows.length - 1])}>Load more</Button>
      )}
    </section>
  );
}
