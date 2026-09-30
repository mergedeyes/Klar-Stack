"use client";

import { useEffect, useState } from "react";
import Image from "next/image";
import { useRouter } from "next/navigation";
import Link from "next/link";
import { Archive, Clock, FileWarning, Gauge, Scale, ShieldAlert, Trash2, X } from "lucide-react";
import { useAuth } from "@/lib/auth-context";
import { adminReportsApi, adminStandingApi, type AdminReport, type Violation } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { SmartBackButton } from "@/components/SmartBackButton";
import { getMediaUrl } from "@/lib/utils/media";
import { REASON_LABELS, SEVERITY_LABELS } from "@/lib/moderation";

const CRITICAL_REASONS = new Set(["csam", "ncii"]);
const HIGH_REASONS = new Set(["violence", "self_harm", "sexual_content", "terrorism"]);

function SeverityBadge({ reason }: { reason: string }) {
  if (CRITICAL_REASONS.has(reason)) {
    return <span className="rounded bg-destructive/15 px-1.5 py-0.5 text-xs font-semibold text-destructive">Critical</span>;
  }
  if (HIGH_REASONS.has(reason)) {
    return <span className="rounded bg-amber-500/15 px-1.5 py-0.5 text-xs font-semibold text-amber-600">High</span>;
  }
  return <span className="rounded bg-muted px-1.5 py-0.5 text-xs text-muted-foreground">Normal</span>;
}

export default function AdminReportsPage() {
  const { user, loading: authLoading } = useAuth();
  const router = useRouter();

  const [reports, setReports] = useState<AdminReport[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [busyId, setBusyId] = useState<string | null>(null);
  // Which report currently has its note field open, and what's typed in
  // it -- keyed by report id so multiple rows don't fight over one input.
  const [noteDrafts, setNoteDrafts] = useState<Record<string, string>>({});
  // The violation catalog, and per report the type picked for a removal
  // (unset: the report reason's first type) and the justification.
  const [violations, setViolations] = useState<Violation[]>([]);
  const [picks, setPicks] = useState<Record<string, string>>({});
  const [justifications, setJustifications] = useState<Record<string, string>>({});

  // The type a removal would be classified as, and whether it needs a
  // justification: another reason than the reporter's, or no strike at all.
  const pickFor = (report: AdminReport) =>
    picks[report.id] ?? violations.find((v) => v.reason === report.reason)?.id ?? "none";
  const needsJustification = (report: AdminReport) => {
    const pick = pickFor(report);
    if (pick === "none") return true;
    return violations.find((v) => v.id === pick)?.reason !== report.reason;
  };

  useEffect(() => {
    if (!authLoading && !user) router.push("/login");
  }, [user, authLoading, router]);

  useEffect(() => {
    if (!user) return;
    adminStandingApi.violations().then(setViolations).catch(() => {});
    adminReportsApi.list()
      .then(setReports)
      .catch((err) => setError(err instanceof Error ? err.message : "Failed to load reports"))
      .finally(() => setLoading(false));
  }, [user]);

  const handleDismiss = async (report: AdminReport) => {
    setBusyId(report.id);
    try {
      await adminReportsApi.dismiss(report.id, noteDrafts[report.id]);
      setReports((prev) => prev.filter((r) => r.id !== report.id));
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to dismiss report");
    } finally {
      setBusyId(null);
    }
  };

  const handleRemove = async (report: AdminReport) => {
    const prompt = report.target_type === "user" || report.evidence_content_deleted
      ? "Confirm this violation? The preserved copy is kept as evidence for the retention period."
      : "Remove this content? This can't be undone.";
    if (!window.confirm(prompt)) return;
    setBusyId(report.id);
    try {
      const classify = report.target_type !== "user" && !report.evidence_content_deleted && violations.length > 0;
      await adminReportsApi.remove(
        report.id,
        noteDrafts[report.id],
        classify ? pickFor(report) : undefined,
        classify ? justifications[report.id] : undefined,
      );
      setReports((prev) => prev.filter((r) => r.id !== report.id));
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to remove content");
    } finally {
      setBusyId(null);
    }
  };

  if (authLoading || !user) return null;

  return (
    <div className="flex-1 bg-background">
      <header className="sticky top-0 z-10 flex h-14 items-center gap-3 border-b border-border bg-background/80 px-4 backdrop-blur">
        <SmartBackButton aria-label="Back" />
        <span className="flex-1 font-semibold">Reports</span>
        <Link href="/admin/rights" className="flex items-center gap-1 text-sm text-muted-foreground hover:text-foreground">
          <FileWarning size={16} /> Claims
        </Link>
        <Link href="/admin/moderation" className="flex items-center gap-1 text-sm text-muted-foreground hover:text-foreground">
          <Scale size={16} /> Objections
        </Link>
        <Link href="/admin/standing" className="flex items-center gap-1 text-sm text-muted-foreground hover:text-foreground">
          <Gauge size={16} /> Standing
        </Link>
        <Link href="/admin/evidence" className="flex items-center gap-1 text-sm text-muted-foreground hover:text-foreground">
          <Archive size={16} /> Evidence
        </Link>
      </header>

      <main className="mx-auto max-w-2xl px-4 py-4">
        {error && (
          <div className="mb-4 rounded-md bg-destructive/10 px-3 py-2 text-sm text-destructive">
            {error}
          </div>
        )}

        {loading && (
          <div className="py-16 text-center text-sm text-muted-foreground animate-pulse">Loading…</div>
        )}

        {!loading && reports.length === 0 && (
          <div className="py-16 text-center">
            <ShieldAlert size={32} className="mx-auto mb-3 text-muted-foreground" />
            <p className="text-sm text-muted-foreground">No pending reports</p>
          </div>
        )}

        <div className="space-y-3">
          {reports.map((report) => (
            <div key={report.id} className="rounded-xl border border-border p-3">
              <div className="mb-2 flex items-center justify-between gap-2">
                <div className="flex items-center gap-2">
                  <SeverityBadge reason={report.reason} />
                  {report.overdue && (
                    <span className="flex items-center gap-1 rounded bg-destructive/15 px-1.5 py-0.5 text-xs font-semibold text-destructive">
                      <Clock size={11} /> 30+ days
                    </span>
                  )}
                  <span className="text-sm font-medium">{REASON_LABELS[report.reason] ?? report.reason}</span>
                </div>
                <span className="text-xs text-muted-foreground">
                  {new Date(report.created_at).toLocaleString()}
                </span>
              </div>

              <p className="mb-2 text-xs text-muted-foreground">
                Reported by <strong>{report.reporter_username ?? "deleted account"}</strong>
                {report.target_username && (
                  <>
                    {" "}·{" "}
                    {report.target_type === "user" ? "account" : report.target_type}{" "}
                    by{" "}
                    <Link href={`/users/${report.target_username}`} className="underline">
                      {report.target_username}
                    </Link>
                  </>
                )}
              </p>

              {(report.target_preview || report.target_thumb_url) && (
                <div className="mb-2 flex items-start gap-2 rounded-md bg-muted/50 p-2">
                  {report.target_thumb_url && (
                    <Image
                      src={getMediaUrl(report.target_thumb_url)}
                      alt=""
                      width={56}
                      height={56}
                      className="h-14 w-14 shrink-0 rounded object-cover"
                      unoptimized
                    />
                  )}
                  {report.target_preview && (
                    <p className="line-clamp-3 text-sm">{report.target_preview}</p>
                  )}
                </div>
              )}

              {/* Likely-illegal reports keep an evidence copy; once the
                  original is gone, that copy is what to review. */}
              {report.evidence_id && (
                <Link
                  href={`/admin/evidence/${report.evidence_id}`}
                  className="mb-2 flex items-center gap-1.5 rounded-md bg-muted/50 p-2 text-sm underline-offset-2 hover:underline"
                >
                  <Archive size={14} />
                  {report.evidence_content_deleted
                    ? "Deleted, preserved as evidence — review it there"
                    : "Evidence copy (as reported, with any edits since)"}
                </Link>
              )}

              {report.details && (
                <p className="mb-2 rounded-md bg-muted/30 p-2 text-sm italic">&ldquo;{report.details}&rdquo;</p>
              )}

              {/* Optional note attached to whichever decision (dismiss or
                  remove) is made below -- e.g. "false report, content is
                  fine" -- stored on the report for future reference. */}
              <input
                value={noteDrafts[report.id] ?? ""}
                onChange={(e) => setNoteDrafts((prev) => ({ ...prev, [report.id]: e.target.value }))}
                placeholder="Add a note for your records (optional)"
                maxLength={1000}
                className="mb-2 w-full rounded-md border border-input bg-transparent px-2 py-1.5 text-sm outline-none placeholder:text-muted-foreground focus:border-ring focus:ring-1 focus:ring-ring"
                disabled={busyId === report.id}
              />

              {/* What the content was, if it is removed: a type from the
                  catalog, each with a written criterion and fixed points,
                  so the choice is "which described case is this", not "how
                  many points". Departing from the reporter's reason or
                  giving no strike needs a justification. */}
              {report.target_type !== "user" && !report.evidence_content_deleted && violations.length > 0 && (() => {
                const pick = pickFor(report);
                const chosen = violations.find((v) => v.id === pick);
                const reasons = [...new Set(violations.map((v) => v.reason))];
                return (
                  <div className="mb-2 space-y-1.5 text-sm">
                    <label className="flex flex-wrap items-center gap-2">
                      <span className="text-muted-foreground">If removed, classify as:</span>
                      <select
                        value={pick}
                        onChange={(e) => setPicks((prev) => ({ ...prev, [report.id]: e.target.value }))}
                        disabled={busyId === report.id}
                        className="min-w-0 max-w-full rounded-md border border-input bg-background px-2 py-1 text-sm"
                      >
                        {/* The report's own reason first. */}
                        {[report.reason, ...reasons.filter((r) => r !== report.reason)].map((reason) => (
                          <optgroup key={reason} label={REASON_LABELS[reason] ?? reason}>
                            {violations.filter((v) => v.reason === reason).map((v) => (
                              <option key={v.id} value={v.id}>
                                {v.label} · {SEVERITY_LABELS[v.severity]}
                                {v.authority_report === "required" && " · report to authorities (required)"}
                                {v.authority_report === "recommended" && " · report to authorities (recommended)"}
                              </option>
                            ))}
                          </optgroup>
                        ))}
                        <option value="none">No strike (needs a justification)</option>
                      </select>
                    </label>
                    {chosen && <p className="text-xs text-muted-foreground">{chosen.criterion_de}</p>}
                    {needsJustification(report) && (
                      <input
                        value={justifications[report.id] ?? ""}
                        onChange={(e) => setJustifications((prev) => ({ ...prev, [report.id]: e.target.value }))}
                        placeholder={
                          pick === "none"
                            ? "Why no strike? (required, internal)"
                            : "Why another reason than the report's? (required, internal)"
                        }
                        maxLength={1000}
                        disabled={busyId === report.id}
                        aria-label="Justification"
                        className="w-full rounded-md border border-input bg-transparent px-2 py-1.5 text-sm outline-none placeholder:text-muted-foreground focus:border-ring focus:ring-1 focus:ring-ring"
                      />
                    )}
                  </div>
                );
              })()}

              <div className="flex gap-2">
                <Button
                  size="sm"
                  variant="outline"
                  onClick={() => handleDismiss(report)}
                  disabled={busyId === report.id}
                >
                  <X size={14} className="mr-1" /> Dismiss
                </Button>
                {report.target_type !== "user" ? (
                  <Button
                    size="sm"
                    variant="destructive"
                    onClick={() => handleRemove(report)}
                    disabled={
                      busyId === report.id ||
                      (!report.evidence_content_deleted && violations.length > 0 &&
                        needsJustification(report) && !justifications[report.id]?.trim())
                    }
                  >
                    {/* Already deleted and preserved: nothing left to
                        remove, but confirming keeps the evidence. */}
                    {report.evidence_content_deleted
                      ? <><ShieldAlert size={14} className="mr-1" /> Confirm violation</>
                      : <><Trash2 size={14} className="mr-1" /> Remove content</>}
                  </Button>
                ) : !report.target_username && (
                  // The account is already deleted: nothing left to remove,
                  // but confirming keeps its preserved profile as evidence
                  // (dismissing would purge it).
                  <Button
                    size="sm"
                    variant="destructive"
                    onClick={() => handleRemove(report)}
                    disabled={busyId === report.id}
                  >
                    <ShieldAlert size={14} className="mr-1" /> Confirm violation
                  </Button>
                )}
              </div>
            </div>
          ))}
        </div>
      </main>
    </div>
  );
}
