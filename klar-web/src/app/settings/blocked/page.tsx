"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { UserX } from "lucide-react";
import { useAuth } from "@/lib/auth-context";
import { blocks, type User } from "@/lib/api";
import { getMediaUrl } from "@/lib/utils/media";
import { Button } from "@/components/ui/button";
import { SmartBackButton } from "@/components/SmartBackButton";

// The accounts the user blocked. Blocked accounts disappear from search
// both ways, so this is where the user finds them again to unblock.
function BlockedRow({ account }: { account: User }) {
  // Kept in the list after unblocking, so a mistaken click can be undone
  // right there.
  const [blocked, setBlocked] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState(false);

  const toggle = async () => {
    setSaving(true);
    setError(false);
    try {
      if (blocked) await blocks.unblock(account.username);
      else await blocks.block(account.username);
      setBlocked(!blocked);
    } catch {
      setError(true);
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="flex items-center gap-3 px-4 py-3">
      <Link href={`/users/${account.username}`} className="flex min-w-0 flex-1 items-center gap-3">
        <div className="h-10 w-10 shrink-0 overflow-hidden rounded-full bg-muted">
          {account.avatar_url ? (
            // eslint-disable-next-line @next/next/no-img-element
            <img src={getMediaUrl(account.avatar_url)} alt="" className="h-full w-full object-cover" />
          ) : (
            <span className="flex h-full w-full items-center justify-center text-sm font-semibold uppercase">
              {account.username[0]}
            </span>
          )}
        </div>
        <div className="min-w-0">
          <p className="truncate text-sm font-semibold">{account.username}</p>
          {account.display_name && (
            <p className="truncate text-xs text-muted-foreground">{account.display_name}</p>
          )}
          {error && <p className="text-xs text-destructive">Couldn&apos;t save — try again</p>}
        </div>
      </Link>
      <Button
        size="sm"
        variant={blocked ? "outline" : "default"}
        onClick={toggle}
        disabled={saving}
        className="shrink-0"
      >
        {blocked ? "Unblock" : "Block"}
      </Button>
    </div>
  );
}

export default function BlockedAccountsPage() {
  const { user, loading: authLoading } = useAuth();
  const router = useRouter();
  const [accounts, setAccounts] = useState<User[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!authLoading && !user) router.push("/login");
  }, [user, authLoading, router]);

  useEffect(() => {
    if (!user) return;
    blocks.list()
      .then(setAccounts)
      .catch((err) => setError(err instanceof Error ? err.message : "Couldn't load blocked accounts"));
  }, [user]);

  if (authLoading || !user) return null;

  return (
    <div className="flex-1 bg-background">
      <header className="sticky top-0 z-10 border-b border-border bg-background/80 backdrop-blur">
        <div className="mx-auto flex h-14 max-w-lg items-center gap-3 px-4">
          <SmartBackButton aria-label="Back" />
          <span className="font-semibold">Blocked accounts</span>
        </div>
      </header>

      <main className="mx-auto max-w-lg px-4 py-4">
        <p className="mb-4 px-1 text-xs text-muted-foreground">
          You and the accounts you block can&apos;t follow each other or interact with each other&apos;s posts, and you
          don&apos;t appear in each other&apos;s search or Discovery. They aren&apos;t told that you blocked them.
        </p>

        {error ? (
          <p className="py-8 text-center text-sm text-destructive">{error}</p>
        ) : accounts === null ? (
          <div className="flex justify-center py-8">
            <div className="h-5 w-5 animate-spin rounded-full border-2 border-muted border-t-foreground" />
          </div>
        ) : accounts.length === 0 ? (
          <div className="py-12 text-center">
            <UserX size={32} className="mx-auto mb-3 text-muted-foreground" />
            <p className="text-sm text-muted-foreground">You haven&apos;t blocked anyone.</p>
          </div>
        ) : (
          <div className="divide-y divide-border overflow-hidden rounded-xl border border-border">
            {accounts.map((account) => (
              <BlockedRow key={account.id} account={account} />
            ))}
          </div>
        )}
      </main>
    </div>
  );
}
