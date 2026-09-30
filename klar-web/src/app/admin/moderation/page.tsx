"use client";

import { useCallback, useEffect, useState } from "react";
import { useRouter } from "next/navigation";
import { ShieldCheck } from "lucide-react";
import { useAuth } from "@/lib/auth-context";
import { adminModerationApi, type AdminModerationDecision } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { SmartBackButton } from "@/components/SmartBackButton";
import { REASON_LABELS } from "@/lib/moderation";

const RESTRICTION_LABELS: Record<AdminModerationDecision["restriction"], string> = {
  removed: "Removed",
  hidden: "Hidden (automatic)",
  flagged: "Warning (automatic)",
};

function DecisionSummary({ d }: { d: AdminModerationDecision }) {
  return (
    <>
      <p className="text-sm font-medium">
        {RESTRICTION_LABELS[d.restriction]} · <span className="capitalize">{d.target_type}</span> ·{" "}
        {REASON_LABELS[d.reason] ?? d.reason}
      </p>
      <p className="text-xs text-muted-foreground">
        {d.affected_username ?? "deleted account"} · {new Date(d.created_at).toLocaleString()}
        {d.superseded && " · superseded"}
        {d.lifted_at && " · lifted"}
      </p>
      {d.content_excerpt && (
        <p className="mt-1 line-clamp-3 rounded bg-muted/50 p-2 text-sm">{d.content_excerpt}</p>
      )}
    </>
  );
}

// Statements of reasons that need an admin: held-back ones (CSAM) waiting
// to be sent, and objections waiting for a response.
export default function AdminModerationPage() {
  const { user, loading: authLoading } = useAuth();
  const router = useRouter();

  const [held, setHeld] = useState<AdminModerationDecision[]>([]);
  const [objections, setObjections] = useState<AdminModerationDecision[]>([]);
  const [loaded, setLoaded] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [responses, setResponses] = useState<Record<string, string>>({});

  useEffect(() => {
    if (!authLoading && !user) router.push("/login");
  }, [user, authLoading, router]);

  const load = useCallback(() => {
    adminModerationApi.queue()
      .then((q) => { setHeld(q.held); setObjections(q.objections); })
      .catch((err) => setError(err instanceof Error ? err.message : "Failed to load"))
      .finally(() => setLoaded(true));
  }, []);

  useEffect(() => {
    if (user) load();
  }, [user, load]);

  const release = async (d: AdminModerationDecision) => {
    if (!window.confirm("Send this statement to the author now? Only do this once the content has been reviewed and, where needed, reported to the authorities.")) return;
    setBusyId(d.id);
    try {
      await adminModerationApi.release(d.id);
      setHeld((prev) => prev.filter((x) => x.id !== d.id));
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to send");
    } finally {
      setBusyId(null);
    }
  };

  const resolve = async (d: AdminModerationDecision, accept: boolean) => {
    setBusyId(d.id);
    try {
      await adminModerationApi.resolveObjection(d.id, accept, responses[d.id] ?? "");
      setObjections((prev) => prev.filter((x) => x.id !== d.id));
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to resolve");
    } finally {
      setBusyId(null);
    }
  };

  if (authLoading || !user) return null;

  return (
    <div className="flex-1 bg-background">
      <header className="sticky top-0 z-10 flex h-14 items-center gap-3 border-b border-border bg-background/80 px-4 backdrop-blur">
        <SmartBackButton aria-label="Back" />
        <span className="font-semibold">Statements &amp; objections</span>
      </header>

      <main className="mx-auto max-w-2xl px-4 py-4">
        {error && (
          <div className="mb-4 rounded-md bg-destructive/10 px-3 py-2 text-sm text-destructive">{error}</div>
        )}
        {!loaded && <div className="py-16 text-center text-sm text-muted-foreground animate-pulse">Loading…</div>}

        {loaded && (
          <>
            <h2 className="mb-1 text-sm font-semibold">Held-back statements</h2>
            <p className="mb-2 text-sm text-muted-foreground">
              Statements for CSAM reports aren&apos;t sent automatically, so a suspect isn&apos;t alerted before
              the content is reviewed and reported. Send each once that&apos;s done.
            </p>
            {held.length === 0 ? (
              <p className="mb-6 text-sm text-muted-foreground">None</p>
            ) : (
              <div className="mb-6 space-y-2">
                {held.map((d) => (
                  <div key={d.id} className="rounded-xl border border-border p-3">
                    <DecisionSummary d={d} />
                    <Button size="sm" variant="outline" className="mt-2" onClick={() => release(d)} disabled={busyId === d.id}>
                      Send statement
                    </Button>
                  </div>
                ))}
              </div>
            )}

            <h2 className="mb-1 text-sm font-semibold">Objections</h2>
            <p className="mb-2 text-sm text-muted-foreground">
              The response is shown to the user. Accepting lifts a hide or warning; removed content can&apos;t be
              restored, so say so in the response.
            </p>
            {objections.length === 0 ? (
              <div className="py-8 text-center">
                <ShieldCheck size={28} className="mx-auto mb-2 text-muted-foreground" />
                <p className="text-sm text-muted-foreground">No open objections</p>
              </div>
            ) : (
              <div className="space-y-2">
                {objections.map((d) => (
                  <div key={d.id} className="rounded-xl border border-border p-3">
                    <DecisionSummary d={d} />
                    <p className="mt-2 text-xs text-muted-foreground">
                      Objection · {d.objected_at && new Date(d.objected_at).toLocaleString()}
                    </p>
                    <p className="mb-2 whitespace-pre-wrap text-sm">{d.objection}</p>
                    <textarea
                      value={responses[d.id] ?? ""}
                      onChange={(e) => setResponses((prev) => ({ ...prev, [d.id]: e.target.value }))}
                      placeholder="Response to the user (required)"
                      maxLength={2000}
                      rows={3}
                      className="mb-2 w-full rounded-md border border-input bg-transparent px-2 py-1.5 text-sm outline-none placeholder:text-muted-foreground focus:border-ring focus:ring-1 focus:ring-ring"
                    />
                    <div className="flex gap-2">
                      <Button size="sm" onClick={() => resolve(d, true)} disabled={busyId === d.id || !responses[d.id]?.trim()}>
                        Accept
                      </Button>
                      <Button size="sm" variant="outline" onClick={() => resolve(d, false)} disabled={busyId === d.id || !responses[d.id]?.trim()}>
                        Reject
                      </Button>
                    </div>
                  </div>
                ))}
              </div>
            )}
          </>
        )}
      </main>
    </div>
  );
}
