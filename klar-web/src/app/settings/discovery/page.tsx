"use client";

import { useEffect } from "react";
import { useRouter } from "next/navigation";
import { useAuth } from "@/lib/auth-context";
import { SmartBackButton } from "@/components/SmartBackButton";
import PersonalizationCard from "@/components/settings/PersonalizationCard";

// Its own page rather than a card on /settings: the consent needs its full
// explanation next to the switch, and /settings has to fit one screen.
export default function DiscoverySettingsPage() {
  const { user, loading: authLoading } = useAuth();
  const router = useRouter();

  useEffect(() => {
    if (!authLoading && !user) router.push("/login");
  }, [user, authLoading, router]);

  if (authLoading || !user) return null;

  return (
    <div className="flex-1 bg-background">
      <header className="sticky top-0 z-10 border-b border-border bg-background/80 backdrop-blur">
        <div className="mx-auto flex h-14 max-w-lg items-center gap-3 px-4">
          <SmartBackButton aria-label="Back" />
          <span className="font-semibold">Discovery</span>
        </div>
      </header>

      <main className="mx-auto max-w-lg px-4 py-2">
        <PersonalizationCard />
      </main>
    </div>
  );
}
