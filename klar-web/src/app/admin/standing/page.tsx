"use client";

import { useEffect, useState } from "react";
import { useRouter } from "next/navigation";
import { Gauge, Search } from "lucide-react";
import { useAuth } from "@/lib/auth-context";
import { adminStandingApi, type AdminStanding } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { SmartBackButton } from "@/components/SmartBackButton";
import { StandingCard } from "@/components/moderation/StandingCard";

// Accounts that reached the warning threshold or are suspended, plus a
// lookup for any other account.
export default function AdminStandingPage() {
  const { user, loading: authLoading } = useAuth();
  const router = useRouter();

  const [list, setList] = useState<AdminStanding[] | null>(null);
  const [lookup, setLookup] = useState("");
  const [looked, setLooked] = useState<AdminStanding | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!authLoading && !user) router.push("/login");
  }, [user, authLoading, router]);

  useEffect(() => {
    if (!user) return;
    adminStandingApi.list()
      .then(setList)
      .catch((err) => setError(err instanceof Error ? err.message : "Failed to load"));
  }, [user]);

  const find = async (e: React.FormEvent) => {
    e.preventDefault();
    const name = lookup.trim().replace(/^@/, "");
    if (!name) return;
    setError(null);
    setLooked(null);
    try {
      setLooked(await adminStandingApi.get(name));
    } catch (err) {
      setError(err instanceof Error ? err.message : "User not found");
    }
  };

  if (authLoading || !user) return null;

  return (
    <div className="flex-1 bg-background">
      <header className="sticky top-0 z-10 flex h-14 items-center gap-3 border-b border-border bg-background/80 px-4 backdrop-blur">
        <SmartBackButton aria-label="Back" />
        <span className="font-semibold">Account standing</span>
      </header>

      <main className="mx-auto max-w-2xl px-4 py-4">
        {error && (
          <div className="mb-4 rounded-md bg-destructive/10 px-3 py-2 text-sm text-destructive">{error}</div>
        )}

        <form onSubmit={find} className="mb-4 flex gap-2">
          <input
            value={lookup}
            onChange={(e) => setLookup(e.target.value)}
            placeholder="Look up an account by username"
            className="min-w-0 flex-1 rounded-md border border-input bg-transparent px-2 py-1.5 text-sm outline-none placeholder:text-muted-foreground focus:border-ring focus:ring-1 focus:ring-ring"
          />
          <Button size="sm" type="submit" variant="outline">
            <Search size={14} className="mr-1" /> Look up
          </Button>
        </form>

        {looked && (
          <section aria-label="Looked-up account" className="mb-6">
            <StandingCard key={looked.user_id} initial={looked} onError={setError} />
          </section>
        )}

        <h2 className="mb-2 text-sm font-semibold">Needs attention</h2>
        {list === null && !error && (
          <div className="py-16 text-center text-sm text-muted-foreground animate-pulse">Loading…</div>
        )}
        {list && list.length === 0 && (
          <div className="py-16 text-center">
            <Gauge size={32} className="mx-auto mb-3 text-muted-foreground" />
            <p className="text-sm text-muted-foreground">No account has reached the warning threshold</p>
          </div>
        )}
        <div className="space-y-3">
          {list?.map((s) => <StandingCard key={s.user_id} initial={s} onError={setError} />)}
        </div>
      </main>
    </div>
  );
}
