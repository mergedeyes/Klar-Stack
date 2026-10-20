"use client";

import { useEffect, useState } from "react";
import { useFormatter, useTranslations } from "next-intl";
import { Archive } from "lucide-react";
import { users } from "@/lib/api";

// Test phase only: everything is deleted before launch unless the owner
// asks to keep their account. Remove with handlers/test_phase.rs once
// Klar has launched.
export default function KeepAccountRow() {
  const t = useTranslations("keepAccount");
  const tc = useTranslations("common");
  const format = useFormatter();
  const [keep, setKeep] = useState<boolean | null>(null);
  const [since, setSince] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    users.keepAccount()
      .then((res) => { setKeep(res.keep); setSince(res.since); })
      .catch(() => setError(t("loadFailed")));
  }, [t]);

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
      setError(err instanceof Error ? err.message : tc("couldntSave"));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="px-4 py-2">
      <label className="flex items-start gap-3">
        <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-full bg-muted">
          <Archive size={16} />
        </span>
        <span className="flex-1">
          <span className="block text-sm font-medium">{t("title")}</span>
          <span className="block text-xs text-muted-foreground">
            {keep && since
              ? t("since", { date: format.dateTime(new Date(since), { dateStyle: "medium" }) })
              : t("info")}
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
