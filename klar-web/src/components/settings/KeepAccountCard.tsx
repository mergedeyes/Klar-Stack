"use client";

import { useEffect, useState } from "react";
import { users } from "@/lib/api";

// Test phase only: everything is deleted before launch unless the owner
// asks to keep their account. Remove with handlers/test_phase.rs once
// Klar has launched.
export default function KeepAccountCard() {
  const [keep, setKeep] = useState<boolean | null>(null);
  const [since, setSince] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    users.keepAccount()
      .then((res) => { setKeep(res.keep); setSince(res.since); })
      .catch(() => setError("Couldn't load this setting"));
  }, []);

  // The box follows the click at once and goes back if saving fails.
  const toggle = async (next: boolean) => {
    setKeep(next);
    setSaving(true);
    setError(null);
    try {
      const res = await users.setKeepAccount(next);
      setKeep(res.keep);
      setSince(res.since);
    } catch (err) {
      setKeep(!next);
      setError(err instanceof Error ? err.message : "Couldn't save");
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="mt-4 rounded-xl border border-border px-4 py-4">
      <label className="flex items-start gap-3">
        <input
          type="checkbox"
          checked={keep ?? false}
          disabled={keep === null}
          aria-busy={saving}
          onChange={(e) => toggle(e.target.checked)}
          className="mt-1"
        />
        <span className="flex-1">
          <span className="block text-sm font-medium">Keep my account after the test</span>
          <span className="mt-1 block text-xs text-muted-foreground">
            Before Klar launches, all test accounts and their data are deleted. Tick this to keep yours: your
            profile, posts and comments stay, and so do follows and chats with others who keep their accounts
            too. You can change your mind until the launch.
          </span>
          {keep && since && (
            <span className="mt-1 block text-xs text-muted-foreground">
              Requested on {new Date(since).toLocaleDateString()}.
            </span>
          )}
        </span>
      </label>
      {error && <p className="mt-2 text-xs text-destructive">{error}</p>}
    </div>
  );
}
