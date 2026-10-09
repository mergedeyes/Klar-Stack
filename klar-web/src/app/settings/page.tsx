"use client";

import { useEffect, useState, type ReactNode } from "react";
import Link from "next/link";
import { useRouter } from "next/navigation";
import {
  ChevronRight,
  Compass,
  Download,
  History,
  KeyRound,
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

// One short line per row and grouped under headings, so the page still fits
// one screen (e2e/layout.spec.ts); anything that needs explaining gets its
// own page.

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

const rowClass = "flex w-full items-center gap-3 px-4 py-2.5 text-left transition-colors hover:bg-muted/50";

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
    if (!next && !window.confirm("Make your account public? Anyone will be able to see your posts.")) return;
    setPendingPrivate(next);
    setSavingPrivacy(true);
    setError(null);
    try {
      await users.updateProfile(null, null, null, next);
      await refreshUser();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Couldn't save");
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
      setError(err instanceof Error ? err.message : "Export failed");
    } finally {
      setExporting(false);
    }
  };

  return (
    <div className="flex-1 bg-background">
      <header className="sticky top-0 z-10 border-b border-border bg-background/80 backdrop-blur">
        <div className="mx-auto flex h-14 max-w-lg items-center gap-3 px-4">
          <SmartBackButton aria-label="Back" />
          <span className="font-semibold">Settings</span>
        </div>
      </header>

      <main className="mx-auto max-w-lg space-y-4 px-4 py-3">
        {error && (
          <div className="rounded-md bg-destructive/10 px-3 py-2 text-sm text-destructive">{error}</div>
        )}

        <Group title="Your account">
          <LinkRow href="/settings/profile" icon={UserPen} label="Edit profile" />
          <LinkRow href="/settings/password" icon={KeyRound} label="Change password" />
          <LinkRow href="/settings/activity" icon={History} label="Your activity" />
        </Group>

        <Group title="Privacy">
          <div className="flex items-center gap-3 px-4 py-2.5">
            <RowContent icon={Lock} label="Private account" />
            <Switch
              checked={isPrivate}
              onCheckedChange={handlePrivacy}
              disabled={savingPrivacy}
              aria-label="Private account"
            />
          </div>
          <LinkRow href="/settings/blocked" icon={UserX} label="Blocked accounts" />
          <LinkRow
            href="/settings/discovery"
            icon={Compass}
            label="Personalised Discovery"
            value={user.personalization_enabled ? "On" : "Off"}
          />
        </Group>

        <Group title="Your data">
          <LinkRow href="/moderation" icon={ShieldCheck} label="Moderation" />
          <button onClick={handleExport} disabled={exporting} className={`${rowClass} disabled:opacity-60`}>
            <RowContent icon={Download} label={exporting ? "Preparing your data…" : "Download your data"} value="ZIP" />
          </button>
          <KeepAccountRow />
        </Group>

        <Link
          href="/settings/account"
          className="block px-1 text-sm text-destructive underline-offset-4 hover:underline"
        >
          Log out or delete account
        </Link>
      </main>
    </div>
  );
}
