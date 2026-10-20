"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { useFormatter, useTranslations } from "next-intl";
import { Ban, ChevronRight, Flag, RotateCcw, ShieldCheck } from "lucide-react";
import { useAuth } from "@/lib/auth-context";
import { moderationApi, standingApi, type ModerationDecision, type MyReport, type MyStanding } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { SmartBackButton } from "@/components/SmartBackButton";
import { StandingScore } from "@/components/moderation/StandingScore";

type ModerationT = ReturnType<typeof useTranslations<"moderation">>;
type ReasonT = ReturnType<typeof useTranslations<"report.reasons">>;

// A report reason's label; the catalog may hold reasons this version of the
// app doesn't know yet, which show as they are.
function reasonLabel(tr: ReasonT, reason: string): string {
  return tr.has(reason as Parameters<ReasonT>[0]) ? tr(reason as Parameters<ReasonT>[0]) : reason;
}

// What came of a report: its outcome once reviewed (DSA Art. 16(5)).
function reportStatus(r: MyReport, t: ModerationT): string {
  if (r.status === "pending") return r.recheck_requested_at ? t("rechecking") : t("underReview");
  return r.outcome ? t(`outcomes.${r.outcome}`) : t("reviewed");
}

// A dismissed report can be sent back once for another look, with a note.
function Recheck({ report, onDone }: { report: MyReport; onDone: (r: MyReport) => void }) {
  const [open, setOpen] = useState(false);
  const [note, setNote] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const t = useTranslations("moderation");
  const tc = useTranslations("common");

  const send = async () => {
    setBusy(true);
    setError(null);
    try {
      onDone(await moderationApi.recheck(report.id, note));
    } catch (err) {
      setError(err instanceof Error ? err.message : t("couldntSend"));
      setBusy(false);
    }
  };

  if (!open) {
    return (
      <button type="button" onClick={() => setOpen(true)} className="mt-1 flex items-center gap-1 text-xs underline">
        <RotateCcw size={12} /> {t("askAgain")}
      </button>
    );
  }
  return (
    <div className="mt-2 space-y-2">
      <textarea
        value={note}
        onChange={(e) => setNote(e.target.value)}
        placeholder={t("recheckPlaceholder")}
        maxLength={2000}
        rows={2}
        className="w-full rounded-md border border-input bg-transparent px-2 py-1.5 text-sm outline-none placeholder:text-muted-foreground focus:border-ring focus:ring-1 focus:ring-ring"
      />
      {error && <p className="text-xs text-destructive">{error}</p>}
      <div className="flex gap-2">
        <Button size="sm" onClick={send} disabled={busy}>{t("recheckSend")}</Button>
        <Button size="sm" variant="outline" onClick={() => setOpen(false)} disabled={busy}>{tc("cancel")}</Button>
      </div>
    </div>
  );
}

// Where you see your account status (score, strikes, suspension), the
// moderation decisions about your content (statements of reasons) and what
// happened to the reports you filed.
export default function ModerationPage() {
  const { user, loading: authLoading } = useAuth();
  const router = useRouter();
  const t = useTranslations("moderation");
  const tr = useTranslations("report.reasons");
  const ts = useTranslations("settings");
  const tc = useTranslations("common");
  const format = useFormatter();
  const dateTime = (iso: string) => format.dateTime(new Date(iso), { dateStyle: "medium", timeStyle: "short" });
  const date = (iso: string) => format.dateTime(new Date(iso), { dateStyle: "medium" });

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
      .catch((err) => setError(err instanceof Error ? err.message : t("loadFailed")));
  }, [user, t]);

  if (authLoading || !user) return null;

  return (
    <div className="flex-1 bg-background">
      <header className="sticky top-safe z-10 flex h-14 items-center gap-3 border-b border-border bg-background/80 px-4 backdrop-blur">
        <SmartBackButton />
        <span className="font-semibold">{ts("moderation")}</span>
      </header>

      <main className="mx-auto max-w-2xl px-4 py-4">
        {error && (
          <div className="mb-4 rounded-md bg-destructive/10 px-3 py-2 text-sm text-destructive">{error}</div>
        )}

        {standing && (
          <section id="standing" className="mb-6 scroll-mt-16">
            <h2 className="mb-2 text-sm font-semibold">{t("accountStatus")}</h2>
            {standing.suspension && (
              <div className="mb-3 flex gap-2 rounded-xl border border-destructive/40 bg-destructive/10 p-3 text-sm">
                <Ban size={18} className="mt-0.5 shrink-0 text-destructive" />
                <p>
                  {standing.suspension.permanent || !standing.suspension.until
                    ? t("suspendedPermanently")
                    : t("suspendedUntil", { date: dateTime(standing.suspension.until) })}{" "}
                  {t("suspendedCan")}
                  {standing.suspension.deletion_at && (
                    <>
                      {" "}
                      {t.rich("deletionNotice", {
                        date: date(standing.suspension.deletion_at),
                        b: (chunks) => <strong>{chunks}</strong>,
                      })}
                    </>
                  )}
                </p>
              </div>
            )}
            <div className="rounded-xl border border-border p-3">
              <StandingScore standing={standing} />
              <p className="mt-2 text-xs text-muted-foreground">{t("pointsExplained")}</p>
              {standing.strikes.length > 0 && (
                <ul className="mt-3 space-y-2 border-t border-border pt-3">
                  {standing.strikes.map((s) => (
                    <li key={s.id}>
                      <Link href={`/moderation/decisions/${s.decision_id}`} className="block text-sm hover:underline">
                        <span className="font-medium">+{s.points}</span>
                        {s.points > s.base_points && <span className="text-muted-foreground"> {t("repeat")}</span>}{" "}
                        {s.violation_label || reasonLabel(tr, s.reason)} ·{" "}
                        <span className="capitalize">{t(`targetsInline.${s.target_type}`)}</span>
                      </Link>
                      {s.content_excerpt && <p className="truncate text-sm text-muted-foreground">{s.content_excerpt}</p>}
                      <p className="text-xs text-muted-foreground">
                        {date(s.created_at)} ·{" "}
                        {s.expires_at ? t("expires", { date: date(s.expires_at) }) : t("noExpiry")}
                      </p>
                    </li>
                  ))}
                </ul>
              )}
            </div>
          </section>
        )}

        <h2 className="mb-2 text-sm font-semibold">{t("decisionsTitle")}</h2>
        {decisions === null && !error ? (
          <div className="py-8 text-center text-sm text-muted-foreground animate-pulse">{tc("loading")}</div>
        ) : decisions && decisions.length === 0 ? (
          <div className="mb-6 rounded-xl border border-border p-4 text-center">
            <ShieldCheck size={28} className="mx-auto mb-2 text-muted-foreground" />
            <p className="text-sm text-muted-foreground">{t("noDecisions")}</p>
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
                    {t(`restrictions.${d.restriction}`)}
                    {d.suspension_days && ` · ${t("days", { count: d.suspension_days })}`} ·{" "}
                    <span className="capitalize">{t(`targetsInline.${d.target_type}`)}</span>
                    {d.lifted_at && <span className="text-muted-foreground"> · {t("lifted")}</span>}
                    {d.objection_status === "pending" && <span className="text-muted-foreground"> · {t("objectionPending")}</span>}
                  </p>
                  {d.content_excerpt && <p className="truncate text-sm text-muted-foreground">{d.content_excerpt}</p>}
                  <p className="text-xs text-muted-foreground">{dateTime(d.created_at)}</p>
                </div>
                <ChevronRight size={16} className="shrink-0 text-muted-foreground" />
              </Link>
            ))}
          </div>
        )}

        <h2 id="reports" className="mb-2 scroll-mt-16 text-sm font-semibold">{t("yourReports")}</h2>
        {reports && reports.length === 0 ? (
          <div className="rounded-xl border border-border p-4 text-center">
            <Flag size={28} className="mx-auto mb-2 text-muted-foreground" />
            <p className="text-sm text-muted-foreground">{t("noReports")}</p>
          </div>
        ) : (
          <div className="space-y-2">
            {reports?.map((r) => (
              <div key={r.id} className="rounded-xl border border-border p-3">
                <p className="text-sm font-medium">
                  {t(`targets.${r.target_type}`)} · {reasonLabel(tr, r.reason)}
                </p>
                <p className="text-sm text-muted-foreground">{reportStatus(r, t)}</p>
                <p className="text-xs text-muted-foreground">
                  {t("reportedAt", { date: dateTime(r.created_at) })}
                  {r.reviewed_at && ` · ${t("reviewedAt", { date: dateTime(r.reviewed_at) })}`}
                </p>
                {r.can_recheck && (
                  <Recheck report={r} onDone={(updated) => setReports((prev) => prev?.map((x) => (x.id === updated.id ? updated : x)) ?? null)} />
                )}
              </div>
            ))}
          </div>
        )}
      </main>
    </div>
  );
}
