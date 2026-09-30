"use client";

import { useCallback, useEffect, useState } from "react";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { Lock, ShieldCheck } from "lucide-react";
import { useAuth } from "@/lib/auth-context";
import { adminLocksApi, type AccountLock } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { SmartBackButton } from "@/components/SmartBackButton";

const inputClass =
  "w-full rounded-md border border-input bg-transparent px-2 py-1.5 text-sm outline-none placeholder:text-muted-foreground focus:border-ring focus:ring-1 focus:ring-ring";

const when = (iso: string) => new Date(iso).toLocaleString();

function LockCard({ lock, onChanged, onError }: {
  lock: AccountLock;
  onChanged: () => void;
  onError: (msg: string) => void;
}) {
  const [assessment, setAssessment] = useState(lock.assessment ?? "");
  const [busy, setBusy] = useState(false);
  const active = !lock.unlocked_at;

  const run = async (action: () => Promise<void>) => {
    setBusy(true);
    try {
      await action();
      onChanged();
    } catch (err) {
      onError(err instanceof Error ? err.message : "Failed");
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="rounded-xl border border-border p-3">
      <div className="mb-1 flex items-center justify-between gap-2">
        {lock.username ? (
          <Link href={`/users/${lock.username}`} className="font-medium hover:underline">@{lock.username}</Link>
        ) : (
          <span className="font-medium text-muted-foreground">Deleted account</span>
        )}
        <span
          className={`rounded px-1.5 py-0.5 text-xs font-semibold ${
            active ? "bg-destructive/15 text-destructive" : "bg-muted text-muted-foreground"
          }`}
        >
          {active ? "Locked" : lock.unlocked_via === "password_reset" ? "Unlocked by new password" : "Unlocked by admin"}
        </span>
      </div>
      <p className="text-xs text-muted-foreground">
        Locked {when(lock.locked_at)} by {lock.locked_by ?? "a deleted account"} · {lock.links_sent} link
        {lock.links_sent === 1 ? "" : "s"} sent, last {when(lock.last_link_sent_at)}
        {lock.unlocked_at && ` · unlocked ${when(lock.unlocked_at)}`}
      </p>
      <p className="mt-2 whitespace-pre-wrap text-sm">{lock.note}</p>

      {/* The incident record (Art. 33(5) GDPR): what the intruder could
          see, the risk, and whether it was reported to the authority. */}
      <label className="mt-3 block text-xs text-muted-foreground" htmlFor={`assessment-${lock.id}`}>
        Assessment{lock.assessed_at && ` (updated ${when(lock.assessed_at)})`}
      </label>
      <textarea
        id={`assessment-${lock.id}`}
        value={assessment}
        onChange={(e) => setAssessment(e.target.value)}
        placeholder="What could the intruder see (DMs, email)? Risk to the person and their contacts? Reported to the data protection authority, and when — or why not?"
        maxLength={2000}
        rows={3}
        disabled={busy}
        className={inputClass}
      />
      <div className="mt-2 flex flex-wrap gap-2">
        <Button
          size="sm"
          variant="outline"
          disabled={busy || !assessment.trim() || assessment === (lock.assessment ?? "")}
          onClick={() => run(() => adminLocksApi.assess(lock.id, assessment))}
        >
          Save assessment
        </Button>
        {active && (
          <Button
            size="sm"
            variant="outline"
            disabled={busy}
            onClick={() => {
              if (window.confirm("Unlock without a new password? Only if the owner proved who they are another way.")) {
                run(() => adminLocksApi.unlock(lock.id));
              }
            }}
          >
            Unlock
          </Button>
        )}
      </div>
    </div>
  );
}

// Locking accounts that look taken over, and the incident log they form.
export default function AdminSecurityPage() {
  const { user, loading: authLoading } = useAuth();
  const router = useRouter();

  const [locks, setLocks] = useState<AccountLock[] | null>(null);
  const [username, setUsername] = useState("");
  const [note, setNote] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [done, setDone] = useState<string | null>(null);

  useEffect(() => {
    if (!authLoading && !user) router.push("/login");
  }, [user, authLoading, router]);

  const load = useCallback(() => {
    adminLocksApi.list()
      .then(setLocks)
      .catch((err) => setError(err instanceof Error ? err.message : "Failed to load"));
  }, []);

  useEffect(() => {
    if (user) load();
  }, [user, load]);

  const lock = async (e: React.FormEvent) => {
    e.preventDefault();
    const name = username.trim().replace(/^@/, "");
    if (!name || !note.trim()) return;
    if (!window.confirm(`Lock @${name}? They're signed out everywhere and get an email to set a new password.`)) return;
    setBusy(true);
    setError(null);
    setDone(null);
    try {
      await adminLocksApi.lock(name, note.trim());
      setDone(`@${name} is locked and has been emailed.`);
      setUsername("");
      setNote("");
      load();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to lock");
    } finally {
      setBusy(false);
    }
  };

  if (authLoading || !user) return null;

  return (
    <div className="flex-1 bg-background">
      <header className="sticky top-0 z-10 flex h-14 items-center gap-3 border-b border-border bg-background/80 px-4 backdrop-blur">
        <SmartBackButton aria-label="Back" />
        <span className="font-semibold">Account security</span>
      </header>

      <main className="mx-auto max-w-2xl px-4 py-4">
        {error && (
          <div className="mb-4 rounded-md bg-destructive/10 px-3 py-2 text-sm text-destructive">{error}</div>
        )}
        {done && <div className="mb-4 rounded-md bg-muted px-3 py-2 text-sm" role="status">{done}</div>}

        <form onSubmit={lock} className="mb-6 space-y-2 rounded-xl border border-border p-3">
          <h2 className="flex items-center gap-1.5 text-sm font-semibold">
            <Lock size={14} /> Lock an account that looks taken over
          </h2>
          <p className="text-xs text-muted-foreground">
            Signs it out everywhere and emails the owner a link to set a new password, which unlocks it. Not a
            moderation measure: no strike, no statement. For spam from a hijacked account, remove the spam with
            &ldquo;No strike&rdquo;.
          </p>
          <input
            value={username}
            onChange={(e) => setUsername(e.target.value)}
            placeholder="Username"
            aria-label="Username"
            disabled={busy}
            className={inputClass}
          />
          <textarea
            value={note}
            onChange={(e) => setNote(e.target.value)}
            placeholder="Why do you suspect a takeover? (required, internal)"
            aria-label="Reason"
            maxLength={2000}
            rows={2}
            disabled={busy}
            className={inputClass}
          />
          <Button size="sm" variant="destructive" type="submit" disabled={busy || !username.trim() || !note.trim()}>
            Lock and email the owner
          </Button>
        </form>

        <h2 className="mb-2 text-sm font-semibold">Incident log</h2>
        {locks === null && !error && (
          <div className="py-16 text-center text-sm text-muted-foreground animate-pulse">Loading…</div>
        )}
        {locks && locks.length === 0 && (
          <div className="py-16 text-center">
            <ShieldCheck size={32} className="mx-auto mb-3 text-muted-foreground" />
            <p className="text-sm text-muted-foreground">No locked accounts so far</p>
          </div>
        )}
        <div className="space-y-3">
          {locks?.map((l) => <LockCard key={l.id} lock={l} onChanged={load} onError={setError} />)}
        </div>
      </main>
    </div>
  );
}
