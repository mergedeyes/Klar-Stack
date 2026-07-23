"use client";

import { useEffect, useState } from "react";
import { useRouter } from "next/navigation";
import { ArrowLeft, ChevronRight, Download, KeyRound, ShieldAlert, Trash2, UserPen } from "lucide-react";
import { useAuth } from "@/lib/auth-context";
import { users } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { SmartBackButton } from '@/components/SmartBackButton';

const sections = [
  {
    href: "/settings/profile",
    icon: UserPen,
    label: "Edit profile",
    description: "Change your avatar, display name, and bio",
  },
  {
    href: "/settings/password",
    icon: KeyRound,
    label: "Change password",
    description: "Update your password",
  },
  {
    href: "/settings/account",
    icon: Trash2,
    label: "Account",
    description: "Log out or delete your account",
    destructive: true,
  },
];

export default function SettingsPage() {
  const { user, loading: authLoading } = useAuth();
  const router = useRouter();
  const [exporting, setExporting] = useState(false);
  const [exportError, setExportError] = useState<string | null>(null);

  useEffect(() => {
    if (!authLoading && !user) router.push("/login");
  }, [user, authLoading, router]);

  if (authLoading || !user) return null;

  // Display-only gate -- actual authorization for /admin/reports is
  // enforced server-side (ADMIN_USER_ID check in reports.rs), this just
  // avoids showing every friends-and-family test account a menu entry
  // they can't use. Requires NEXT_PUBLIC_ADMIN_USER_ID to be set to the
  // same value as the backend's ADMIN_USER_ID.
  const isAdmin = !!process.env.NEXT_PUBLIC_ADMIN_USER_ID && user.id === process.env.NEXT_PUBLIC_ADMIN_USER_ID;

  const visibleSections = isAdmin
    ? [
        ...sections,
        {
          href: "/admin/reports",
          icon: ShieldAlert,
          label: "Reports",
          description: "Review reported content",
        },
      ]
    : sections;

  const handleExport = async () => {
    setExporting(true);
    setExportError(null);
    try {
      await users.exportData();
    } catch (err) {
      setExportError(err instanceof Error ? err.message : "Export failed");
    } finally {
      setExporting(false);
    }
  };

  return (
    <div className="min-h-screen bg-background">
      <header className="sticky top-0 z-10 border-b border-border bg-background/80 backdrop-blur">
        <div className="mx-auto flex h-14 max-w-lg items-center gap-3 px-4">
          <SmartBackButton aria-label="Back" />
          <span className="font-semibold">Account</span>
        </div>
      </header>

      <main className="mx-auto max-w-lg px-4 py-4">
        <div className="overflow-hidden rounded-xl border border-border">
          {visibleSections.map((section, i) => (
            <button
              key={section.href}
              onClick={() => router.push(section.href)}
              className={`flex w-full items-center gap-4 px-4 py-4 text-left transition-colors hover:bg-muted/50 ${
                i < visibleSections.length - 1 ? "border-b border-border" : ""
              }`}
            >
              <div className={`flex h-9 w-9 shrink-0 items-center justify-center rounded-full ${
                section.destructive ? "bg-destructive/10 text-destructive" : "bg-muted"
              }`}>
                <section.icon size={18} />
              </div>
              <div className="flex-1">
                <p className={`text-sm font-medium ${section.destructive ? "text-destructive" : ""}`}>
                  {section.label}
                </p>
                <p className="text-xs text-muted-foreground">{section.description}</p>
              </div>
              <ChevronRight size={16} className="text-muted-foreground" />
            </button>
          ))}
        </div>

        {/* Right of access / data portability (Art. 15 + 20 DSGVO) — a
            separate action rather than a sub-page, since it's a single
            direct download rather than something with its own screen. */}
        <div className="mt-4 overflow-hidden rounded-xl border border-border">
          <button
            onClick={handleExport}
            disabled={exporting}
            className="flex w-full items-center gap-4 px-4 py-4 text-left transition-colors hover:bg-muted/50 disabled:opacity-60"
          >
            <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-full bg-muted">
              <Download size={18} />
            </div>
            <div className="flex-1">
              <p className="text-sm font-medium">
                {exporting ? "Preparing your data…" : "Download your data"}
              </p>
              <p className="text-xs text-muted-foreground">
                Get everything Klar has stored about your account as a JSON file
              </p>
            </div>
          </button>
        </div>
        {exportError && (
          <p className="mt-2 px-1 text-xs text-destructive">{exportError}</p>
        )}
      </main>
    </div>
  );
}
