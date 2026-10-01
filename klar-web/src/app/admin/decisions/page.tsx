"use client";

import { useCallback, useEffect, useState } from "react";
import { useRouter } from "next/navigation";
import Link from "next/link";
import { History } from "lucide-react";
import { useAuth } from "@/lib/auth-context";
import { adminModerationApi, type DecisionLogFilter, type LoggedDecision } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { SmartBackButton } from "@/components/SmartBackButton";
import { REASON_LABELS } from "@/lib/moderation";

const inputClass =
  "rounded-md border border-input bg-background px-2 py-1 text-sm outline-none placeholder:text-muted-foreground focus:border-ring focus:ring-1 focus:ring-ring";

const RESTRICTIONS = ["removed", "hidden", "flagged", "warning", "suspended", "banned"];
const SOURCES: Record<string, string> = {
  notice: "Report or notice",
  own_initiative: "Own initiative",
  authority_order: "Authority order",
  rights_claim: "Rights claim",
};
const PAGE = 50;

// Every moderation decision, newest first: who decided what, when and why,
// for checking the team's own work. Read-only; justifications and the
// content itself stay behind the logged strike and evidence views.
export default function DecisionLogPage() {
  const { user, loading: authLoading } = useAuth();
  const router = useRouter();
  const [filter, setFilter] = useState<DecisionLogFilter>({});
  const [rows, setRows] = useState<LoggedDecision[]>([]);
  const [more, setMore] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!authLoading && !user) router.push("/login");
  }, [user, authLoading, router]);

  const load = useCallback((after?: LoggedDecision) => {
    adminModerationApi
      .decisions({ ...filter, limit: PAGE, before_time: after?.created_at, before_id: after?.id })
      .then((page) => {
        setRows((prev) => (after ? [...prev, ...page] : page));
        setMore(page.length === PAGE);
      })
      .catch((err) => setError(err instanceof Error ? err.message : "Failed to load"));
  }, [filter]);

  useEffect(() => {
    if (user) load();
  }, [user, load]);

  const set = (key: keyof DecisionLogFilter, value: string) =>
    setFilter((prev) => ({ ...prev, [key]: value === "" ? undefined : key === "automated" ? value === "true" : value }));

  if (authLoading || !user) return null;

  return (
    <div className="flex-1 bg-background">
      <header className="sticky top-0 z-10 flex h-14 items-center gap-3 border-b border-border bg-background/80 px-4 backdrop-blur">
        <SmartBackButton aria-label="Back" />
        <span className="font-semibold">Decision log</span>
      </header>
      <main className="mx-auto max-w-3xl px-4 py-4">
        {error && <div className="mb-4 rounded-md bg-destructive/10 px-3 py-2 text-sm text-destructive">{error}</div>}

        <div className="mb-4 flex flex-wrap gap-2">
          <select aria-label="Restriction" className={inputClass} onChange={(e) => set("restriction", e.target.value)}>
            <option value="">Any decision</option>
            {RESTRICTIONS.map((r) => <option key={r} value={r}>{r}</option>)}
          </select>
          <select aria-label="Reason" className={inputClass} onChange={(e) => set("reason", e.target.value)}>
            <option value="">Any reason</option>
            {Object.entries(REASON_LABELS).map(([r, label]) => <option key={r} value={r}>{label}</option>)}
          </select>
          <select aria-label="Source" className={inputClass} onChange={(e) => set("source", e.target.value)}>
            <option value="">Any source</option>
            {Object.entries(SOURCES).map(([s, label]) => <option key={s} value={s}>{label}</option>)}
          </select>
          <select aria-label="Automated" className={inputClass} onChange={(e) => set("automated", e.target.value)}>
            <option value="">Automatic and by the team</option>
            <option value="true">Automatic only</option>
            <option value="false">By the team only</option>
          </select>
          <input aria-label="Decided by" placeholder="Decided by (username)" className={inputClass} onBlur={(e) => set("decided_by", e.target.value.trim())} />
          <input aria-label="Affected account" placeholder="Affected account" className={inputClass} onBlur={(e) => set("affected", e.target.value.trim())} />
        </div>

        {rows.length === 0 && !error && (
          <div className="py-16 text-center">
            <History size={32} className="mx-auto mb-3 text-muted-foreground" />
            <p className="text-sm text-muted-foreground">No decisions</p>
          </div>
        )}

        <ul className="space-y-2">
          {rows.map((d) => (
            <li key={d.id} className="rounded-lg border border-border p-2 text-sm">
              <div className="flex flex-wrap items-center gap-2">
                <span className="font-medium">{d.restriction}</span>
                <span>{d.target_type}</span>
                <span className="text-muted-foreground">{REASON_LABELS[d.reason] ?? d.reason}</span>
                {d.violation_type && <span className="text-muted-foreground">· {d.violation_type}</span>}
                <span className="ml-auto text-xs text-muted-foreground">{new Date(d.created_at).toLocaleString()}</span>
              </div>
              <p className="text-xs text-muted-foreground">
                {d.automated ? "Automatic" : `By ${d.decided_by_username ?? "a former admin"}`}
                {d.source && ` · ${SOURCES[d.source] ?? d.source}`}
                {d.report_count > 0 && ` · ${d.report_count} report${d.report_count === 1 ? "" : "s"}`}
                {" · "}
                {d.affected_username ? (
                  <Link href={`/admin/standing/${d.affected_username}`} className="underline">@{d.affected_username}</Link>
                ) : "account deleted"}
                {!d.delivered_at && " · statement held back"}
                {d.lifted_at && ` · lifted ${new Date(d.lifted_at).toLocaleDateString()}`}
                {d.superseded && " · replaced"}
                {d.objection_status && ` · objection ${d.objection_status}`}
              </p>
              {d.content_excerpt && <p className="mt-1 line-clamp-2 text-xs">{d.content_excerpt}</p>}
            </li>
          ))}
        </ul>
        {more && (
          <div className="mt-4 text-center">
            <Button variant="outline" size="sm" onClick={() => load(rows[rows.length - 1])}>Load more</Button>
          </div>
        )}
      </main>
    </div>
  );
}
