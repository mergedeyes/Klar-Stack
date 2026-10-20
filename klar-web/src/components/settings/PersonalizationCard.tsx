"use client";

import { useState } from "react";
import Link from "next/link";
import { useTranslations } from "next-intl";
import { useAuth } from "@/lib/auth-context";
import { users } from "@/lib/api";
import { Switch } from "@/components/ui/switch";

// The consent to the interaction log that will rank Discovery
// (handlers/events.rs): off until the account switches it on. The home
// feed stays chronological either way.
export default function PersonalizationCard() {
  const { user, refreshUser } = useAuth();
  const t = useTranslations("personalization");
  const ts = useTranslations("settings");
  const tc = useTranslations("common");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // The switch follows the click at once and goes back if saving fails.
  const [pending, setPending] = useState<boolean | null>(null);
  const enabled = pending ?? user?.personalization_enabled ?? false;

  const toggle = async () => {
    const next = !enabled;
    if (!next && !window.confirm(t("confirmOff"))) {
      return;
    }
    setPending(next);
    setSaving(true);
    setError(null);
    try {
      await users.setPersonalization(next);
      await refreshUser();
    } catch (err) {
      setError(err instanceof Error ? err.message : tc("couldntSave"));
    } finally {
      setPending(null);
      setSaving(false);
    }
  };

  return (
    <div className="mt-4 rounded-xl border border-border px-4 py-4">
      <div className="flex items-start justify-between gap-4">
        <div>
          <p className="text-sm font-medium" id="personalization-label">{ts("discovery")}</p>
          <p className="mt-1 text-xs text-muted-foreground">
            {t.rich("explanation", {
              link: (chunks) => <Link href="/datenschutz#discovery" className="underline">{chunks}</Link>,
            })}
          </p>
        </div>
        <Switch
          checked={enabled}
          onCheckedChange={toggle}
          aria-labelledby="personalization-label"
          disabled={saving || !user}
        />
      </div>
      {error && <p className="mt-2 text-xs text-destructive">{error}</p>}
    </div>
  );
}
