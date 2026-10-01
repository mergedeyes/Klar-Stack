"use client";

import { useCallback, useEffect, useState } from "react";
import { useRouter } from "next/navigation";
import { BadgeCheck } from "lucide-react";
import { useAuth } from "@/lib/auth-context";
import { adminOfficialAccountsApi, type OfficialAccount, type OfficialRename } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { SmartBackButton } from "@/components/SmartBackButton";

const inputClass =
  "w-full rounded-md border border-input bg-transparent px-2 py-1.5 text-sm outline-none placeholder:text-muted-foreground focus:border-ring focus:ring-1 focus:ring-ring";

// Official accounts are the ones with a verified @klarsocial.eu address. The
// sign-up form and the profile settings refuse staff names like "Klar", so
// an official account registers under a temporary name and gets its real
// one here. The backend skips the reserved names and the 14-day cooldown
// for these accounts only, and logs every rename with its reason.
export default function AdminOfficialAccountsPage() {
  const { user, loading: authLoading } = useAuth();
  const router = useRouter();

  const [accounts, setAccounts] = useState<OfficialAccount[] | null>(null);
  const [renames, setRenames] = useState<OfficialRename[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [done, setDone] = useState<string | null>(null);

  const load = useCallback(() => {
    adminOfficialAccountsApi.list()
      .then((res) => {
        setAccounts(res.accounts);
        setRenames(res.renames);
      })
      .catch((err) => setError(err instanceof Error ? err.message : "Failed to load"));
  }, []);

  useEffect(() => {
    if (!authLoading && !user) router.push("/login");
  }, [user, authLoading, router]);

  useEffect(() => {
    if (user) load();
  }, [user, load]);

  if (authLoading || !user) return null;

  return (
    <div className="flex-1 bg-background">
      <header className="sticky top-0 z-10 flex h-14 items-center gap-3 border-b border-border bg-background/80 px-4 backdrop-blur">
        <SmartBackButton aria-label="Back" />
        <span className="font-semibold">Official accounts</span>
      </header>

      <main className="mx-auto max-w-2xl px-4 py-4">
        {error && <div className="mb-4 rounded-md bg-destructive/10 px-3 py-2 text-sm text-destructive">{error}</div>}
        {done && <div className="mb-4 rounded-md bg-muted px-3 py-2 text-sm" role="status">{done}</div>}

        <div className="mb-6 space-y-2 rounded-xl border border-border p-3 text-sm">
          <h2 className="flex items-center gap-1.5 font-semibold">
            <BadgeCheck size={14} /> How it works
          </h2>
          <p className="text-muted-foreground">
            Register the account with its @klarsocial.eu address under a temporary name and verify the address. It then
            shows up here, and you can give it any name, including the ones nobody else can take (Klar, support, admin,
            …). Every rename is logged below with its reason.
          </p>
        </div>

        <h2 className="mb-2 text-sm font-semibold">Accounts</h2>
        {accounts === null && !error && (
          <div className="py-8 text-center text-sm text-muted-foreground animate-pulse">Loading…</div>
        )}
        {accounts?.length === 0 && (
          <p className="mb-6 text-sm text-muted-foreground">
            No account with a verified @klarsocial.eu address yet.
          </p>
        )}
        <div className="mb-6 space-y-3">
          {accounts?.map((a) => (
            <RenameCard
              key={a.id}
              account={a}
              onRenamed={(from, to) => {
                setError(null);
                setDone(`@${from} is now @${to}.`);
                load();
              }}
              onError={(msg) => {
                setDone(null);
                setError(msg);
              }}
            />
          ))}
        </div>

        <h2 className="mb-2 text-sm font-semibold">Rename log</h2>
        {renames.length === 0 && <p className="text-sm text-muted-foreground">No renames so far.</p>}
        <div className="space-y-2">
          {renames.map((r) => (
            <div key={r.id} className="rounded-xl border border-border p-3 text-sm">
              <p className="font-medium">
                @{r.old_username} → @{r.new_username}
                {r.user_id === null && <span className="font-normal text-muted-foreground"> · account deleted</span>}
              </p>
              <p className="text-xs text-muted-foreground">
                {new Date(r.renamed_at).toLocaleString()} · by {r.renamed_by ? `@${r.renamed_by}` : "a deleted admin"}
              </p>
              <p className="mt-1 whitespace-pre-wrap">{r.reason}</p>
            </div>
          ))}
        </div>
      </main>
    </div>
  );
}

function RenameCard({
  account,
  onRenamed,
  onError,
}: {
  account: OfficialAccount;
  onRenamed: (from: string, to: string) => void;
  onError: (message: string) => void;
}) {
  const [username, setUsername] = useState("");
  const [reason, setReason] = useState("");
  const [busy, setBusy] = useState(false);

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    const name = username.trim().replace(/^@/, "");
    if (!name || !reason.trim()) return;
    if (!window.confirm(`Rename @${account.username} to @${name}? Links to the old name stop working.`)) return;
    setBusy(true);
    try {
      await adminOfficialAccountsApi.rename(account.id, name, reason.trim());
      setUsername("");
      setReason("");
      onRenamed(account.username, name);
    } catch (err) {
      onError(err instanceof Error ? err.message : "Failed to rename");
    } finally {
      setBusy(false);
    }
  };

  return (
    <form onSubmit={submit} className="space-y-2 rounded-xl border border-border p-3 text-sm">
      <div>
        <p className="font-medium">@{account.username}</p>
        <p className="text-xs text-muted-foreground">
          {account.email} · registered {new Date(account.created_at).toLocaleDateString()}
        </p>
      </div>
      <input
        value={username}
        onChange={(e) => setUsername(e.target.value)}
        placeholder="New username"
        aria-label={`New username for @${account.username}`}
        maxLength={30}
        disabled={busy}
        className={inputClass}
      />
      <textarea
        value={reason}
        onChange={(e) => setReason(e.target.value)}
        placeholder="Why? (required, kept in the log)"
        aria-label={`Reason for renaming @${account.username}`}
        maxLength={2000}
        rows={2}
        disabled={busy}
        className={inputClass}
      />
      <Button size="sm" type="submit" disabled={busy || !username.trim() || !reason.trim()}>
        Rename
      </Button>
    </form>
  );
}
