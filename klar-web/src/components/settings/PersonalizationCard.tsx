"use client";

import { useState } from "react";
import Link from "next/link";
import { useAuth } from "@/lib/auth-context";
import { users } from "@/lib/api";

// The opt-out for the interaction log that ranks Discovery
// (handlers/events.rs). The home feed stays chronological either way.
export default function PersonalizationCard() {
  const { user, refreshUser } = useAuth();
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // The switch follows the click at once and goes back if saving fails.
  const [pending, setPending] = useState<boolean | null>(null);
  const enabled = pending ?? user?.personalization_enabled ?? true;

  const toggle = async () => {
    const next = !enabled;
    if (!next && !window.confirm("Switch off personalised Discovery? The likes and comments stored for it are deleted.")) {
      return;
    }
    setPending(next);
    setSaving(true);
    setError(null);
    try {
      await users.setPersonalization(next);
      await refreshUser();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Couldn't save");
    } finally {
      setPending(null);
      setSaving(false);
    }
  };

  return (
    <div className="mt-4 rounded-xl border border-border px-4 py-4">
      <div className="flex items-start justify-between gap-4">
        <div>
          <p className="text-sm font-medium" id="personalization-label">Personalised Discovery</p>
          <p className="mt-1 text-xs text-muted-foreground">
            Discovery can show posts that fit what you like and comment on. For that, Klar stores which posts you
            like, unlike or comment on, and deletes each month&apos;s entries once they are 12 months old. Your home
            feed is always chronological. Switching this off deletes what was stored.{" "}
            <Link href="/datenschutz#discovery" className="underline">Privacy policy</Link>
          </p>
        </div>
        <button
          type="button"
          role="switch"
          aria-checked={enabled}
          aria-labelledby="personalization-label"
          onClick={toggle}
          disabled={saving || !user}
          className={`relative h-6 w-11 shrink-0 rounded-full border transition-colors disabled:opacity-60 ${
            enabled ? "border-primary bg-primary" : "border-border bg-input"
          }`}
        >
          <span
            className={`absolute left-0.5 top-0.5 h-5 w-5 rounded-full border border-border/50 bg-white shadow transition-transform ${
              enabled ? "translate-x-5" : "translate-x-0"
            }`}
          />
        </button>
      </div>
      {error && <p className="mt-2 text-xs text-destructive">{error}</p>}
    </div>
  );
}
