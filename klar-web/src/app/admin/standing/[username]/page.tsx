"use client";

import { Suspense, useEffect, useState } from "react";
import { useParams, useRouter, useSearchParams } from "next/navigation";
import { useAuth } from "@/lib/auth-context";
import { adminStandingApi, type AdminStanding } from "@/lib/api";
import { SmartBackButton } from "@/components/SmartBackButton";
import { StandingCard } from "@/components/moderation/StandingCard";

// One account's standing, where the report queue sends reports about an
// account: a measure or a removal from the profile decided here answers
// them (?reports=<id>,<id>).
function AccountStandingPage() {
  const { user, loading: authLoading } = useAuth();
  const router = useRouter();
  const { username } = useParams<{ username: string }>();
  const searchParams = useSearchParams();
  const reportIds = (searchParams.get("reports") ?? "").split(",").filter(Boolean);

  const [standing, setStanding] = useState<AdminStanding | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!authLoading && !user) router.push("/login");
  }, [user, authLoading, router]);

  useEffect(() => {
    if (!user) return;
    adminStandingApi.get(username)
      .then(setStanding)
      .catch((err) => setError(err instanceof Error ? err.message : "Failed to load"));
  }, [user, username]);

  if (authLoading || !user) return null;

  return (
    <div className="flex-1 bg-background">
      <header className="sticky top-0 z-10 flex h-14 items-center gap-3 border-b border-border bg-background/80 px-4 backdrop-blur">
        <SmartBackButton aria-label="Back" />
        <span className="font-semibold">@{username}</span>
      </header>
      <main className="mx-auto max-w-2xl px-4 py-4">
        {error && <div className="mb-4 rounded-md bg-destructive/10 px-3 py-2 text-sm text-destructive">{error}</div>}
        {!standing && !error && (
          <div className="py-16 text-center text-sm text-muted-foreground animate-pulse">Loading…</div>
        )}
        {standing && <StandingCard initial={standing} onError={setError} reportIds={reportIds} />}
      </main>
    </div>
  );
}

export default function Page() {
  return (
    <Suspense fallback={null}>
      <AccountStandingPage />
    </Suspense>
  );
}
