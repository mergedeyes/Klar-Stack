"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { Ban, ChevronRight, Flag, ShieldCheck } from "lucide-react";
import { useAuth } from "@/lib/auth-context";
import { moderationApi, standingApi, type ModerationDecision, type MyReport, type MyStanding } from "@/lib/api";
import { SmartBackButton } from "@/components/SmartBackButton";
import { REASON_LABELS } from "@/lib/moderation";
import { StandingScore } from "@/components/moderation/StandingScore";

const RESTRICTION_LABELS: Record<ModerationDecision["restriction"], string> = {
  removed: "Removed",
  hidden: "Hidden",
  flagged: "Shown with a warning",
  warning: "Warning",
  suspended: "Suspended",
  banned: "Suspended permanently",
};

const REPORT_STATUS: Record<MyReport["status"], string> = {
  pending: "Under review",
  dismissed: "Reviewed — no violation found",
  actioned: "Reviewed — action taken",
};

// Where you see your account status (score, strikes, suspension), the
// moderation decisions about your content (statements of reasons) and what
// happened to the reports you filed.
export default function ModerationPage() {
  const { user, loading: authLoading } = useAuth();
  const router = useRouter();

  const [decisions, setDecisions] = useState<ModerationDecision[] | null>(null);
  const [reports, setReports] = useState<MyReport[] | null>(null);
  const [standing, setStanding] = useState<MyStanding | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!authLoading && !user) router.push("/login");
  }, [user, authLoading, router]);

  useEffect(() => {
    if (!user) return;
    Promise.all([moderationApi.myDecisions(), moderationApi.myReports(), standingApi.mine()])
      .then(([d, r, s]) => { setDecisions(d); setReports(r); setStanding(s); })
      .catch((err) => setError(err instanceof Error ? err.message : "Failed to load"));
  }, [user]);

  if (authLoading || !user) return null;

  return (
    <div className="flex-1 bg-background">
      <header className="sticky top-0 z-10 flex h-14 items-center gap-3 border-b border-border bg-background/80 px-4 backdrop-blur">
        <SmartBackButton aria-label="Back" />
        <span className="font-semibold">Moderation</span>
      </header>

      <main className="mx-auto max-w-2xl px-4 py-4">
        {error && (
          <div className="mb-4 rounded-md bg-destructive/10 px-3 py-2 text-sm text-destructive">{error}</div>
        )}

        {standing && (
          <section id="standing" className="mb-6 scroll-mt-16">
            <h2 className="mb-2 text-sm font-semibold">Account status</h2>
            {standing.suspension && (
              <div className="mb-3 flex gap-2 rounded-xl border border-destructive/40 bg-destructive/10 p-3 text-sm">
                <Ban size={18} className="mt-0.5 shrink-0 text-destructive" />
                <p>
                  {standing.suspension.permanent || !standing.suspension.until
                    ? "Your account is permanently suspended."
                    : `Your account is suspended until ${new Date(standing.suspension.until).toLocaleString()}.`}{" "}
                  You can still read, object to the decision below, export your data and delete your account. Your
                  profile and content are hidden from others.
                </p>
              </div>
            )}
            <div className="rounded-xl border border-border p-3">
              <StandingScore standing={standing} />
              <p className="mt-2 text-xs text-muted-foreground">
                Each confirmed violation adds points by how serious it was; they expire after a while. Warnings and
                suspensions are always decided by a person, never automatically.
              </p>
              {standing.strikes.length > 0 && (
                <ul className="mt-3 space-y-2 border-t border-border pt-3">
                  {standing.strikes.map((s) => (
                    <li key={s.id}>
                      <Link href={`/moderation/decisions/${s.decision_id}`} className="block text-sm hover:underline">
                        <span className="font-medium">+{s.points}</span>
                        {s.points > s.base_points && <span className="text-muted-foreground"> (repeat, ×1.5)</span>}{" "}
                        {s.violation_label || (REASON_LABELS[s.reason] ?? s.reason)} ·{" "}
                        <span className="capitalize">{s.target_type}</span>
                      </Link>
                      {s.content_excerpt && <p className="truncate text-sm text-muted-foreground">{s.content_excerpt}</p>}
                      <p className="text-xs text-muted-foreground">
                        {new Date(s.created_at).toLocaleDateString()} ·{" "}
                        {s.expires_at ? `expires ${new Date(s.expires_at).toLocaleDateString()}` : "doesn't expire"}
                      </p>
                    </li>
                  ))}
                </ul>
              )}
            </div>
          </section>
        )}

        <h2 className="mb-2 text-sm font-semibold">Decisions about your content and account</h2>
        {decisions === null && !error ? (
          <div className="py-8 text-center text-sm text-muted-foreground animate-pulse">Loading…</div>
        ) : decisions && decisions.length === 0 ? (
          <div className="mb-6 rounded-xl border border-border p-4 text-center">
            <ShieldCheck size={28} className="mx-auto mb-2 text-muted-foreground" />
            <p className="text-sm text-muted-foreground">No decisions about your content</p>
          </div>
        ) : (
          <div className="mb-6 space-y-2">
            {decisions?.map((d) => (
              <Link
                key={d.id}
                href={`/moderation/decisions/${d.id}`}
                className="flex items-center gap-3 rounded-xl border border-border p-3 hover:bg-muted/40"
              >
                <div className="min-w-0 flex-1">
                  <p className="text-sm font-medium">
                    {RESTRICTION_LABELS[d.restriction]}
                    {d.suspension_days && ` · ${d.suspension_days} days`} ·{" "}
                    <span className="capitalize">{d.target_type === "user" ? "account" : d.target_type}</span>
                    {d.lifted_at && <span className="text-muted-foreground"> · lifted</span>}
                    {d.objection_status === "pending" && <span className="text-muted-foreground"> · objection pending</span>}
                  </p>
                  {d.content_excerpt && <p className="truncate text-sm text-muted-foreground">{d.content_excerpt}</p>}
                  <p className="text-xs text-muted-foreground">{new Date(d.created_at).toLocaleString()}</p>
                </div>
                <ChevronRight size={16} className="shrink-0 text-muted-foreground" />
              </Link>
            ))}
          </div>
        )}

        <h2 id="reports" className="mb-2 scroll-mt-16 text-sm font-semibold">Your reports</h2>
        {reports && reports.length === 0 ? (
          <div className="rounded-xl border border-border p-4 text-center">
            <Flag size={28} className="mx-auto mb-2 text-muted-foreground" />
            <p className="text-sm text-muted-foreground">You haven&apos;t reported anything</p>
          </div>
        ) : (
          <div className="space-y-2">
            {reports?.map((r) => (
              <div key={r.id} className="rounded-xl border border-border p-3">
                <p className="text-sm font-medium">
                  <span className="capitalize">{r.target_type}</span> · {REASON_LABELS[r.reason] ?? r.reason}
                </p>
                <p className="text-sm text-muted-foreground">{REPORT_STATUS[r.status]}</p>
                <p className="text-xs text-muted-foreground">
                  Reported {new Date(r.created_at).toLocaleString()}
                  {r.reviewed_at && ` · reviewed ${new Date(r.reviewed_at).toLocaleString()}`}
                </p>
              </div>
            ))}
          </div>
        )}
      </main>
    </div>
  );
}
