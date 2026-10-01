"use client";

import { useEffect, useState } from "react";
import { useRouter } from "next/navigation";
import { Archive, ChevronRight, Download, Gauge, History, KeyRound, Lock, Scale, ShieldAlert, ShieldCheck, Trash2, UserPen, type LucideIcon } from "lucide-react";
import { BadgeCheck, FileText, FileWarning } from "lucide-react";
import { useAuth } from "@/lib/auth-context";
import { adminModerationApi, users, type AdminAttention } from "@/lib/api";
import { MessageSquareText } from "lucide-react";
import { SmartBackButton } from '@/components/SmartBackButton';
import KeepAccountCard from "@/components/settings/KeepAccountCard";

interface SettingsSection {
  href: string;
  icon: LucideIcon;
  label: string;
  description: string;
  destructive?: boolean;
  // Admin entries: how much is waiting there, and whether any of it is
  // urgent.
  badge?: number;
  urgent?: boolean;
}

const sections: SettingsSection[] = [
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
    href: "/moderation",
    icon: ShieldCheck,
    label: "Moderation",
    description: "Account status, decisions about your content, and your reports",
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
  // What waits for an admin, as badges on the admin entries.
  const [attention, setAttention] = useState<AdminAttention | null>(null);

  useEffect(() => {
    if (!authLoading && !user) router.push("/login");
  }, [user, authLoading, router]);

  useEffect(() => {
    if (user?.is_admin) adminModerationApi.attention().then(setAttention).catch(() => {});
  }, [user?.is_admin]);

  if (authLoading || !user) return null;

  // Display-only gate -- actual authorization for /admin/reports is
  // enforced server-side (ADMIN_EMAILS check in reports.rs). The backend
  // computes is_admin itself (GET /users/me, see utils::is_admin_email)
  // and just hands us a boolean -- no email/ID list to keep in sync on
  // the frontend, and no separate NEXT_PUBLIC_* build-time var needed.
  const visibleSections: SettingsSection[] = user.is_admin
    ? [
        ...sections,
        {
          href: "/admin/reports",
          icon: ShieldAlert,
          label: "Reports",
          description: "Review reported content",
          badge: attention?.reports,
          urgent: !!attention?.urgent_reports,
        },
        {
          href: "/admin/moderation",
          icon: Scale,
          label: "Statements & objections",
          description: "Held-back statements and objections waiting for an answer",
          badge: attention ? attention.objections + attention.held_statements : undefined,
          urgent: !!attention?.overdue_held_statements,
        },
        {
          href: "/admin/decisions",
          icon: History,
          label: "Decision log",
          description: "Every moderation decision: who, what, when, why",
        },
        {
          href: "/admin/standing",
          icon: Gauge,
          label: "Account standing",
          description: "Scores, warnings and suspensions",
        },
        {
          href: "/admin/security",
          icon: Lock,
          label: "Account security",
          description: "Lock accounts that look taken over; incident log",
        },
        {
          href: "/admin/official",
          icon: BadgeCheck,
          label: "Official accounts",
          description: "Give @klarsocial.eu accounts staff names like Klar",
        },
        {
          href: "/admin/legal-updates",
          icon: FileText,
          label: "Legal updates",
          description: "Notices about changed Terms and privacy policy",
        },
        {
          href: "/admin/rights",
          icon: FileWarning,
          label: "Rights claims",
          description: "Copyright and other rights notices",
          badge: attention?.rights_claims,
        },
        {
          href: "/admin/evidence",
          icon: Archive,
          label: "Evidence",
          description: "Preserved copies of deleted, likely-illegal content",
          badge: attention ? attention.overdue_evidence + attention.authority_reports : undefined,
          urgent: !!attention?.authority_reports,
        },
        {
          href: "/admin/feedback",
          icon: MessageSquareText,
          label: "Feedback",
          description: "Bug reports and ideas from testers",
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
    <div className="flex-1 bg-background">
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
              {!!section.badge && (
                <span
                  aria-label={`${section.badge} waiting`}
                  className={`rounded-full px-2 py-0.5 text-xs font-semibold ${
                    section.urgent ? "bg-destructive text-white" : "bg-muted"
                  }`}
                >
                  {section.badge}
                </span>
              )}
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
                Get everything Klar has stored about your account, including your photos, as a ZIP file
              </p>
            </div>
          </button>
        </div>
        {exportError && (
          <p className="mt-2 px-1 text-xs text-destructive">{exportError}</p>
        )}

        <KeepAccountCard />
      </main>
    </div>
  );
}
