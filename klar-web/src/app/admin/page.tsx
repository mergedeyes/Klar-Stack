"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import { useRouter } from "next/navigation";
import {
  Archive,
  BadgeCheck,
  ChevronRight,
  FileDown,
  FileText,
  FileWarning,
  Gauge,
  History,
  Lock,
  MessageSquareText,
  Scale,
  ShieldAlert,
  type LucideIcon,
} from "lucide-react";
import { useAuth } from "@/lib/auth-context";
import { adminModerationApi, type AdminAttention } from "@/lib/api";
import { SmartBackButton } from "@/components/SmartBackButton";

interface ToolPage {
  href: string;
  icon: LucideIcon;
  label: string;
  description: string;
  // How much is waiting on the page, and whether any of it is urgent.
  waiting?: (a: AdminAttention) => number;
  urgent?: (a: AdminAttention) => boolean;
}

// Only the overview pages are listed. The detail pages (a report's case,
// an account's standing or review, an evidence record) open from these, so
// they never need a place of their own here.
const CATEGORIES: { title: string; pages: ToolPage[] }[] = [
  {
    title: "Moderation",
    pages: [
      {
        href: "/admin/reports",
        icon: ShieldAlert,
        label: "Reports",
        description: "Review reported content",
        waiting: (a) => a.reports,
        urgent: (a) => a.urgent_reports > 0,
      },
      {
        href: "/admin/moderation",
        icon: Scale,
        label: "Statements & objections",
        description: "Held-back statements and objections waiting for an answer",
        waiting: (a) => a.objections + a.held_statements,
        urgent: (a) => a.overdue_held_statements > 0,
      },
      {
        href: "/admin/rights",
        icon: FileWarning,
        label: "Rights claims",
        description: "Copyright and other rights notices",
        waiting: (a) => a.rights_claims,
      },
      {
        href: "/admin/decisions",
        icon: History,
        label: "Decision log",
        description: "Every moderation decision: who, what, when, why",
      },
    ],
  },
  {
    title: "Accounts",
    pages: [
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
    ],
  },
  {
    title: "Legal",
    pages: [
      {
        href: "/admin/evidence",
        icon: Archive,
        label: "Evidence",
        description: "Preserved copies of deleted, likely-illegal content",
        waiting: (a) => a.overdue_evidence + a.authority_reports,
        urgent: (a) => a.authority_reports > 0,
      },
      {
        href: "/admin/legal-updates",
        icon: FileText,
        label: "Legal updates",
        description: "Notices about changed Terms and privacy policy",
      },
      {
        href: "/admin/audit",
        icon: FileDown,
        label: "Audit export",
        description: "A period's moderation records for an authority or a transparency report",
      },
    ],
  },
  {
    title: "Testing",
    pages: [
      {
        href: "/admin/feedback",
        icon: MessageSquareText,
        label: "Feedback",
        description: "Bug reports and ideas from testers",
      },
    ],
  },
];

export default function AdminPage() {
  const { user, loading: authLoading } = useAuth();
  const router = useRouter();
  const [attention, setAttention] = useState<AdminAttention | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!authLoading && !user) router.push("/login");
  }, [user, authLoading, router]);

  useEffect(() => {
    if (!user?.is_admin) return;
    adminModerationApi.attention()
      .then(setAttention)
      .catch((err) => setError(err instanceof Error ? err.message : "Failed to load what's waiting"));
  }, [user?.is_admin]);

  if (authLoading || !user) return null;

  // What needs someone now, urgent first, so the queues with work in them
  // are the first thing an admin sees. The categories below list everything.
  const waiting = attention
    ? CATEGORIES.flatMap((c) => c.pages)
        .map((page) => ({ page, count: page.waiting?.(attention) ?? 0, urgent: !!page.urgent?.(attention) }))
        .filter((w) => w.count > 0)
        .sort((a, b) => Number(b.urgent) - Number(a.urgent))
    : [];

  return (
    <div className="flex-1 bg-background">
      <header className="sticky top-0 z-10 border-b border-border bg-background/80 backdrop-blur">
        <div className="mx-auto flex h-14 max-w-lg items-center gap-3 px-4">
          <SmartBackButton aria-label="Back" />
          <span className="font-semibold">Moderation tools</span>
        </div>
      </header>

      <main className="mx-auto max-w-lg space-y-6 px-4 py-4">
        {/* Display-only gate, like the header button: the API refuses
            everyone else on every page linked here. */}
        {!user.is_admin ? (
          <div className="rounded-md bg-destructive/10 px-3 py-2 text-sm text-destructive">Admin access required</div>
        ) : (
          <>
            {error && (
              <div className="rounded-md bg-destructive/10 px-3 py-2 text-sm text-destructive">{error}</div>
            )}

            <section aria-labelledby="waiting">
              <h2 id="waiting" className="mb-2 text-xs font-semibold uppercase tracking-wide text-muted-foreground">
                Waiting for you
              </h2>
              {!attention && !error && (
                <p className="text-sm text-muted-foreground animate-pulse">Checking the queues…</p>
              )}
              {attention && waiting.length === 0 && (
                <p className="text-sm text-muted-foreground">Nothing waiting.</p>
              )}
              {waiting.length > 0 && (
                <div className="overflow-hidden rounded-xl border border-border">
                  {waiting.map(({ page, count, urgent }) => (
                    <ToolRow key={page.href} page={page} count={count} urgent={urgent} />
                  ))}
                </div>
              )}
            </section>

            {CATEGORIES.map((category) => (
              <section key={category.title} aria-labelledby={`category-${category.title}`}>
                <h2
                  id={`category-${category.title}`}
                  className="mb-2 text-xs font-semibold uppercase tracking-wide text-muted-foreground"
                >
                  {category.title}
                </h2>
                <div className="overflow-hidden rounded-xl border border-border">
                  {category.pages.map((page) => (
                    <ToolRow
                      key={page.href}
                      page={page}
                      count={attention ? page.waiting?.(attention) : undefined}
                      urgent={attention ? page.urgent?.(attention) : undefined}
                    />
                  ))}
                </div>
              </section>
            ))}
          </>
        )}
      </main>
    </div>
  );
}

function ToolRow({ page, count, urgent }: { page: ToolPage; count?: number; urgent?: boolean }) {
  return (
    <Link
      href={page.href}
      className="flex items-center gap-4 border-b border-border px-4 py-4 transition-colors last:border-0 hover:bg-muted/50"
    >
      <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-full bg-muted">
        <page.icon size={18} />
      </div>
      <div className="min-w-0 flex-1">
        <p className="text-sm font-medium">{page.label}</p>
        <p className="text-xs text-muted-foreground">{page.description}</p>
      </div>
      {!!count && (
        <span
          aria-label={`${count} waiting`}
          className={`rounded-full px-2 py-0.5 text-xs font-semibold ${urgent ? "bg-destructive text-white" : "bg-muted"}`}
        >
          {count}
        </span>
      )}
      <ChevronRight size={16} className="shrink-0 text-muted-foreground" />
    </Link>
  );
}
