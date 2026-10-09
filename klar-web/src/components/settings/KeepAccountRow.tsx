"use client";

import { useEffect, useState } from "react";
import { Archive } from "lucide-react";
import { users } from "@/lib/api";

// Test phase only: everything is deleted before launch unless the owner
// asks to keep their account. Remove with handlers/test_phase.rs once
// Klar has launched.
export default function KeepAccountRow() {
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
    <div className="px-4 py-2.5">
      <label className="flex items-start gap-3">
        <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-full bg-muted">
          <Archive size={16} />
        </span>
        <span className="flex-1">
          <span className="block text-sm font-medium">Keep my account after the test</span>
          <span className="block text-xs text-muted-foreground">
            {keep && since
              ? `Requested on ${new Date(since).toLocaleDateString()}. `
              : "Test accounts are deleted before launch. "}
            Yours keeps its posts and comments, and follows and chats with others who keep theirs.
          </span>
        </span>
        <input
          type="checkbox"
          checked={keep ?? false}
          disabled={keep === null}
          aria-busy={saving}
          onChange={(e) => toggle(e.target.checked)}
          className="mt-2 h-4 w-4"
        />
      </label>
      {error && <p className="mt-2 text-xs text-destructive">{error}</p>}
    </div>
  );
}
