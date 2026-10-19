"use client";

import { useEffect, useState, type ReactNode } from "react";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { useLocale, useTranslations } from "next-intl";
import {
  ChevronRight,
  Compass,
  Download,
  History,
  KeyRound,
  Languages,
  Lock,
  ShieldCheck,
  UserPen,
  UserX,
  type LucideIcon,
} from "lucide-react";
import { useAuth } from "@/lib/auth-context";
import { users } from "@/lib/api";
import { SmartBackButton } from "@/components/SmartBackButton";
import { Switch } from "@/components/ui/switch";
import KeepAccountRow from "@/components/settings/KeepAccountRow";
import { LOCALE_NAMES, isLocale } from "@/i18n/config";

// One short line per row and grouped under headings, so the page still fits
// one screen (e2e/layout.spec.ts; the desktop one is 800px high, with little
// to spare); anything that needs explaining gets its own page.

function Group({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section aria-label={title}>
      <h2 className="mb-1 px-1 text-xs font-medium text-muted-foreground">{title}</h2>
      <div className="divide-y divide-border overflow-hidden rounded-xl border border-border">{children}</div>
    </section>
  );
}

function RowContent({ icon: Icon, label, value }: { icon: LucideIcon; label: string; value?: string }) {
  return (
    <>
      <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-full bg-muted">
        <Icon size={16} />
      </span>
      <span className="flex-1 text-sm font-medium">{label}</span>
      {value && <span className="text-xs text-muted-foreground">{value}</span>}
    </>
  );
}

const rowClass = "flex w-full items-center gap-3 px-4 py-2 text-left transition-colors hover:bg-muted/50";

function LinkRow({ href, icon, label, value }: { href: string; icon: LucideIcon; label: string; value?: string }) {
  return (
    <Link href={href} className={rowClass}>
      <RowContent icon={icon} label={label} value={value} />
      <ChevronRight size={16} className="text-muted-foreground" />
    </Link>
  );
}

export default function SettingsPage() {
  const { user, loading: authLoading, refreshUser } = useAuth();
  const router = useRouter();
  const t = useTranslations("settings");
  const tc = useTranslations("common");
  const locale = useLocale();
  const [exporting, setExporting] = useState(false);
  const [savingPrivacy, setSavingPrivacy] = useState(false);
  // The switch follows the click at once and goes back if saving fails.
  const [pendingPrivate, setPendingPrivate] = useState<boolean | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!authLoading && !user) router.push("/login");
  }, [user, authLoading, router]);

  if (authLoading || !user) return null;

  const isPrivate = pendingPrivate ?? user.is_private ?? false;

  const handlePrivacy = async (next: boolean) => {
    if (!next && !window.confirm(t("confirmPublic"))) return;
    setPendingPrivate(next);
    setSavingPrivacy(true);
    setError(null);
    try {
      await users.updateProfile(null, null, null, next);
      await refreshUser();
    } catch (err) {
      setError(err instanceof Error ? err.message : tc("couldntSave"));
    } finally {
      setPendingPrivate(null);
      setSavingPrivacy(false);
    }
  };

  // Right of access / data portability (Art. 15 + 20 DSGVO): a direct
  // download rather than a page of its own.
  const handleExport = async () => {
    setExporting(true);
    setError(null);
    try {
      await users.exportData();
    } catch (err) {
      setError(err instanceof Error ? err.message : t("exportFailed"));
    } finally {
      setExporting(false);
    }
  };

  return (
    <div className="flex-1 bg-background">
      <header className="sticky top-safe z-10 border-b border-border bg-background/80 backdrop-blur">
        <div className="mx-auto flex h-14 max-w-lg items-center gap-3 px-4">
          <SmartBackButton aria-label={tc("back")} />
          <span className="font-semibold">{t("title")}</span>
        </div>
      </header>

      <main className="mx-auto max-w-lg space-y-4 px-4 py-3">
        {error && (
          <div className="rounded-md bg-destructive/10 px-3 py-2 text-sm text-destructive">{error}</div>
        )}

        <Group title={t("yourAccount")}>
          <LinkRow href="/settings/profile" icon={UserPen} label={t("editProfile")} />
          <LinkRow href="/settings/password" icon={KeyRound} label={t("changePassword")} />
          <LinkRow href="/settings/activity" icon={History} label={t("activity")} />
          <LinkRow href="/settings/appearance" icon={Languages} label={t("appearance")} value={LOCALE_NAMES[isLocale(locale) ? locale : "en"]} />
        </Group>

        <Group title={t("privacy")}>
          <div className="flex items-center gap-3 px-4 py-2">
            <RowContent icon={Lock} label={t("privateAccount")} />
            <Switch
              checked={isPrivate}
              onCheckedChange={handlePrivacy}
              disabled={savingPrivacy}
              aria-label={t("privateAccount")}
            />
          </div>
          <LinkRow href="/settings/blocked" icon={UserX} label={t("blocked")} />
          <LinkRow
            href="/settings/discovery"
            icon={Compass}
            label={t("discovery")}
            value={user.personalization_enabled ? tc("on") : tc("off")}
          />
        </Group>

        <Group title={t("yourData")}>
          <LinkRow href="/moderation" icon={ShieldCheck} label={t("moderation")} />
          <button onClick={handleExport} disabled={exporting} className={`${rowClass} disabled:opacity-60`}>
            <RowContent icon={Download} label={exporting ? t("preparing") : t("download")} value="ZIP" />
          </button>
          <KeepAccountRow />
        </Group>

        <Link
          href="/settings/account"
          className="block px-1 text-sm text-destructive underline-offset-4 hover:underline"
        >
          {t("logOutOrDelete")}
        </Link>
      </main>
    </div>
  );
}
