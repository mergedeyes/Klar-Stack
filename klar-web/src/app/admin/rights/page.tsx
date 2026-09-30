"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { Clock, FileWarning } from "lucide-react";
import { useAuth } from "@/lib/auth-context";
import { adminRightsApi, type AdminRightsClaim, type RightsClaimStatus } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { SmartBackButton } from "@/components/SmartBackButton";

const TYPE_LABELS: Record<AdminRightsClaim["claim_type"], string> = {
  copyright: "Copyright",
  trademark: "Trademark",
  other: "Other right",
};

const STATUS_LABELS: Record<RightsClaimStatus, string> = {
  submitted: "New",
  triaged: "In review",
  evidence_requested: "Waiting for claimant",
  accepted: "Accepted (post hidden)",
  declined: "Declined",
  restored: "Restored after objection",
};

const inputClass =
  "w-full rounded-md border border-input bg-transparent px-2 py-1.5 text-sm outline-none placeholder:text-muted-foreground focus:border-ring focus:ring-1 focus:ring-ring";

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <p className="break-words text-sm">
      <span className="text-muted-foreground">{label}: </span>
      {children}
    </p>
  );
}

// Formal rights claims (copyright etc.), separate from user reports. Open
// claims are listed oldest first.
export default function AdminRightsPage() {
  const { user, loading: authLoading } = useAuth();
  const router = useRouter();

  const [claims, setClaims] = useState<AdminRightsClaim[]>([]);
  const [showAll, setShowAll] = useState(false);
  // The filter the loaded list belongs to; loading is derived from it.
  const [loadedFor, setLoadedFor] = useState<boolean | null>(null);
  const loading = loadedFor !== showAll;
  const [reloadKey, setReloadKey] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [messages, setMessages] = useState<Record<string, string>>({});

  useEffect(() => {
    if (!authLoading && !user) router.push("/login");
  }, [user, authLoading, router]);

  useEffect(() => {
    if (!user) return;
    let cancelled = false;
    adminRightsApi.list(showAll)
      .then((data) => { if (!cancelled) { setClaims(data); setError(null); } })
      .catch((err) => { if (!cancelled) setError(err instanceof Error ? err.message : "Failed to load"); })
      .finally(() => { if (!cancelled) setLoadedFor(showAll); });
    return () => { cancelled = true; };
  }, [user, showAll, reloadKey]);

  const act = async (claim: AdminRightsClaim, action: () => Promise<void>, confirmText?: string) => {
    if (confirmText && !window.confirm(confirmText)) return;
    setBusyId(claim.id);
    setError(null);
    try {
      await action();
      setMessages((prev) => ({ ...prev, [claim.id]: "" }));
      setReloadKey((k) => k + 1);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Action failed");
    } finally {
      setBusyId(null);
    }
  };

  if (authLoading || !user) return null;

  return (
    <div className="flex-1 bg-background">
      <header className="sticky top-0 z-10 flex h-14 items-center gap-3 border-b border-border bg-background/80 px-4 backdrop-blur">
        <SmartBackButton aria-label="Back" />
        <span className="flex-1 font-semibold">Rights claims</span>
        <label className="flex items-center gap-2 text-sm text-muted-foreground">
          <input type="checkbox" checked={showAll} onChange={(e) => setShowAll(e.target.checked)} />
          Show closed
        </label>
      </header>

      <main className="mx-auto max-w-2xl px-4 py-4">
        {error && <div className="mb-4 rounded-md bg-destructive/10 px-3 py-2 text-sm text-destructive">{error}</div>}
        {loading && <div className="py-16 text-center text-sm text-muted-foreground animate-pulse">Loading…</div>}
        {!loading && claims.length === 0 && (
          <div className="py-16 text-center">
            <FileWarning size={32} className="mx-auto mb-3 text-muted-foreground" />
            <p className="text-sm text-muted-foreground">No open rights claims</p>
          </div>
        )}

        {!loading && (
          <div className="space-y-3">
            {claims.map((c) => {
              const open = c.status === "submitted" || c.status === "triaged" || c.status === "evidence_requested";
              const message = messages[c.id] ?? "";
              return (
                <div key={c.id} className="space-y-2 rounded-xl border border-border p-3">
                  <div className="flex flex-wrap items-center gap-2">
                    <span className="text-sm font-medium">{TYPE_LABELS[c.claim_type]}</span>
                    <span className="rounded bg-muted px-1.5 py-0.5 text-xs">{STATUS_LABELS[c.status]}</span>
                    {c.overdue && (
                      <span className="flex items-center gap-1 rounded bg-destructive/15 px-1.5 py-0.5 text-xs font-semibold text-destructive">
                        <Clock size={11} /> 30+ days
                      </span>
                    )}
                    <span className="text-xs text-muted-foreground">{new Date(c.created_at).toLocaleString()}</span>
                  </div>

                  <Row label="Claimant">
                    {c.claimant_name} &lt;{c.claimant_email}&gt;
                    {c.claimant_organization && `, ${c.claimant_organization}`}
                    {c.claimant_username && ` (Klar: ${c.claimant_username})`}
                    {c.represented_party && ` — on behalf of ${c.represented_party}`}
                  </Row>
                  <Row label="Post">
                    {c.target_exists ? (
                      <>
                        <Link href={`/posts/${c.target_id}`} className="underline">open</Link>
                        {c.target_username && ` by ${c.target_username}`}
                        {c.target_caption && ` — “${c.target_caption}”`}
                      </>
                    ) : (
                      "no longer exists"
                    )}
                  </Row>
                  <Row label="Work"><span className="whitespace-pre-wrap">{c.work_description}</span></Row>
                  <Row label="Basis of rights"><span className="whitespace-pre-wrap">{c.ownership_basis}</span></Row>
                  {c.original_url && <Row label="Original">{c.original_url}</Row>}
                  {c.evidence_request && <Row label="We asked"><span className="whitespace-pre-wrap">{c.evidence_request}</span></Row>}
                  {c.claimant_response && <Row label="They answered"><span className="whitespace-pre-wrap">{c.claimant_response}</span></Row>}
                  {c.decision_reason && <Row label="Decline reason"><span className="whitespace-pre-wrap">{c.decision_reason}</span></Row>}

                  {open && (
                    <>
                      <textarea
                        value={message}
                        onChange={(e) => setMessages((prev) => ({ ...prev, [c.id]: e.target.value }))}
                        placeholder="Message to the claimant (for an evidence request or a decline; they see it)"
                        maxLength={4000}
                        rows={2}
                        className={inputClass}
                      />
                      <div className="flex flex-wrap gap-2">
                        {c.status === "submitted" && (
                          <Button size="sm" variant="outline" disabled={busyId === c.id}
                            onClick={() => act(c, () => adminRightsApi.triage(c.id))}>
                            Take on
                          </Button>
                        )}
                        <Button size="sm" variant="outline" disabled={busyId === c.id || !message.trim()}
                          onClick={() => act(c, () => adminRightsApi.requestEvidence(c.id, message))}>
                          Ask for evidence
                        </Button>
                        <Button size="sm" variant="outline" disabled={busyId === c.id || !message.trim()}
                          onClick={() => act(c, () => adminRightsApi.decline(c.id, message))}>
                          Decline
                        </Button>
                        <Button size="sm" variant="destructive" disabled={busyId === c.id || !c.target_exists}
                          onClick={() => act(c, () => adminRightsApi.accept(c.id),
                            "Accept the claim? The post is hidden and its author gets a statement of reasons they can object to.")}>
                          Accept &amp; hide post
                        </Button>
                      </div>
                    </>
                  )}
                </div>
              );
            })}
          </div>
        )}
      </main>
    </div>
  );
}
