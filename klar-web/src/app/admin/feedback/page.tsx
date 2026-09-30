"use client";

import { useEffect, useState } from "react";
import { useRouter } from "next/navigation";
import { Bug, Lightbulb, MessageCircle, MessageSquareText } from "lucide-react";
import { useAuth } from "@/lib/auth-context";
import { feedbackApi, type FeedbackEntry } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { SmartBackButton } from "@/components/SmartBackButton";

const CATEGORY = {
  bug: { label: "Bug", icon: Bug },
  idea: { label: "Idea", icon: Lightbulb },
  other: { label: "Other", icon: MessageCircle },
} as const;

const STATUS_LABELS: Record<FeedbackEntry["status"], string> = { new: "New", seen: "Seen", done: "Done" };

// Tester feedback, newest first. "Open" hides what's marked done.
export default function AdminFeedbackPage() {
  const { user, loading: authLoading } = useAuth();
  const router = useRouter();

  const [entries, setEntries] = useState<FeedbackEntry[]>([]);
  const [showAll, setShowAll] = useState(false);
  // The filter the loaded list belongs to; loading is derived from it.
  const [loadedFor, setLoadedFor] = useState<boolean | null>(null);
  const loading = loadedFor !== showAll;
  const [error, setError] = useState<string | null>(null);
  const [notes, setNotes] = useState<Record<string, string>>({});
  const [busyId, setBusyId] = useState<string | null>(null);

  useEffect(() => {
    if (!authLoading && !user) router.push("/login");
  }, [user, authLoading, router]);

  useEffect(() => {
    if (!user) return;
    let cancelled = false;
    feedbackApi.list(showAll)
      .then((data) => { if (!cancelled) { setEntries(data); setError(null); } })
      .catch((err) => { if (!cancelled) setError(err instanceof Error ? err.message : "Failed to load"); })
      .finally(() => { if (!cancelled) setLoadedFor(showAll); });
    return () => { cancelled = true; };
  }, [user, showAll]);

  const setStatus = async (entry: FeedbackEntry, status: FeedbackEntry["status"]) => {
    setBusyId(entry.id);
    const note = notes[entry.id] ?? entry.admin_note ?? "";
    try {
      await feedbackApi.update(entry.id, status, note || null);
      setEntries((prev) =>
        status === "done" && !showAll
          ? prev.filter((e) => e.id !== entry.id)
          : prev.map((e) => (e.id === entry.id ? { ...e, status, admin_note: note || null } : e))
      );
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to update");
    } finally {
      setBusyId(null);
    }
  };

  if (authLoading || !user) return null;

  return (
    <div className="flex-1 bg-background">
      <header className="sticky top-0 z-10 flex h-14 items-center gap-3 border-b border-border bg-background/80 px-4 backdrop-blur">
        <SmartBackButton aria-label="Back" />
        <span className="flex-1 font-semibold">Feedback</span>
        <label className="flex items-center gap-2 text-sm text-muted-foreground">
          <input type="checkbox" checked={showAll} onChange={(e) => setShowAll(e.target.checked)} />
          Show done
        </label>
      </header>

      <main className="mx-auto max-w-2xl px-4 py-4">
        {error && (
          <div className="mb-4 rounded-md bg-destructive/10 px-3 py-2 text-sm text-destructive">{error}</div>
        )}
        {loading && <div className="py-16 text-center text-sm text-muted-foreground animate-pulse">Loading…</div>}
        {!loading && !error && entries.length === 0 && (
          <div className="py-16 text-center">
            <MessageSquareText size={32} className="mx-auto mb-3 text-muted-foreground" />
            <p className="text-sm text-muted-foreground">No open feedback</p>
          </div>
        )}

        {!loading && (
          <div className="space-y-3">
            {entries.map((entry) => {
              const { label, icon: Icon } = CATEGORY[entry.category];
              return (
                <div key={entry.id} className="rounded-xl border border-border p-3">
                  <div className="mb-1 flex flex-wrap items-center gap-2 text-sm">
                    <span className="flex items-center gap-1 font-medium"><Icon size={14} /> {label}</span>
                    <span className={`rounded px-1.5 py-0.5 text-xs ${entry.status === "new" ? "bg-amber-500/15 font-semibold text-amber-600" : "bg-muted text-muted-foreground"}`}>
                      {STATUS_LABELS[entry.status]}
                    </span>
                    <span className="text-xs text-muted-foreground">
                      {entry.username ?? "deleted account"} · {new Date(entry.created_at).toLocaleString()}
                    </span>
                  </div>
                  <p className="mb-2 whitespace-pre-wrap break-words text-sm">{entry.message}</p>
                  {(entry.page_path || entry.viewport || entry.user_agent) && (
                    <p className="mb-2 break-words text-xs text-muted-foreground">
                      {[entry.page_path && `Page ${entry.page_path}`, entry.viewport && `Screen ${entry.viewport}`, entry.user_agent]
                        .filter(Boolean).join(" · ")}
                    </p>
                  )}
                  <input
                    value={notes[entry.id] ?? entry.admin_note ?? ""}
                    onChange={(e) => setNotes((prev) => ({ ...prev, [entry.id]: e.target.value }))}
                    placeholder="Note (e.g. link to the issue)"
                    maxLength={1000}
                    className="mb-2 w-full rounded-md border border-input bg-transparent px-2 py-1.5 text-sm outline-none placeholder:text-muted-foreground focus:border-ring focus:ring-1 focus:ring-ring"
                  />
                  <div className="flex gap-2">
                    {entry.status !== "seen" && (
                      <Button size="sm" variant="outline" onClick={() => setStatus(entry, "seen")} disabled={busyId === entry.id}>
                        Mark seen
                      </Button>
                    )}
                    {entry.status !== "done" ? (
                      <Button size="sm" onClick={() => setStatus(entry, "done")} disabled={busyId === entry.id}>Done</Button>
                    ) : (
                      <Button size="sm" variant="outline" onClick={() => setStatus(entry, "new")} disabled={busyId === entry.id}>Reopen</Button>
                    )}
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </main>
    </div>
  );
}
