"use client";

import { useEffect, useState } from "react";
import Image from "next/image";
import { useRouter } from "next/navigation";
import Link from "next/link";
import { Archive, Clock, FileWarning, Gauge, History, Plus, RotateCcw, Scale, ShieldAlert, Trash2, X } from "lucide-react";
import { useAuth } from "@/lib/auth-context";
import { adminReportsApi, adminStandingApi, type QueueReport, type ReportGroup, type Violation } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { SmartBackButton } from "@/components/SmartBackButton";
import { getMediaUrl } from "@/lib/utils/media";
import { ACCOUNT_REVIEW_REASONS, REASON_LABELS, SEVERITY_LABELS, SOURCE_LABELS } from "@/lib/moderation";

function SeverityBadge({ severity }: { severity: ReportGroup["severity"] }) {
  if (severity === "critical") {
    return <span className="rounded bg-destructive/15 px-1.5 py-0.5 text-xs font-semibold text-destructive">Critical</span>;
  }
  if (severity === "high") {
    return <span className="rounded bg-amber-500/15 px-1.5 py-0.5 text-xs font-semibold text-amber-600">High</span>;
  }
  return <span className="rounded bg-muted px-1.5 py-0.5 text-xs text-muted-foreground">Normal</span>;
}

const TARGET_LABELS: Record<ReportGroup["target_type"], string> = {
  post: "Post",
  comment: "Comment",
  user: "Account",
  message: "Direct message",
};

const STATUS_LABELS: Record<NonNullable<ReportGroup["target_status"]>, string | null> = {
  visible: null,
  flagged: "Shown behind a warning",
  hidden: "Hidden",
  removed: "Removed",
};

// A group's key: one card per reported item.
const keyOf = (group: ReportGroup) => `${group.target_type}:${group.target_id}`;

function Reporter({ report }: { report: QueueReport }) {
  if (report.source === "public_notice") {
    return (
      <>
        Public notice from{" "}
        <strong>
          {report.notifier_name ?? "an anonymous notifier"}
          {report.notifier_email && ` <${report.notifier_email}>`}
        </strong>
      </>
    );
  }
  if (report.source === "authority_order") {
    return (
      <>
        Order from <strong>{report.authority}</strong>
        {report.order_reference && <> (ref. {report.order_reference})</>}, entered by {report.reporter_username ?? "a former admin"}
      </>
    );
  }
  if (report.source === "own_initiative") {
    return <>Opened by the team ({report.reporter_username ?? "a former admin"})</>;
  }
  return <>Reported by <strong>{report.reporter_username ?? "deleted account"}</strong></>;
}

export default function AdminReportsPage() {
  const { user, loading: authLoading } = useAuth();
  const router = useRouter();

  const [groups, setGroups] = useState<ReportGroup[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  // Per card (keyOf): the internal note, the answer to a pending
  // objection, the violation type picked for a removal (unset: the first
  // reason's first type) and the justification for departing from it.
  const [notes, setNotes] = useState<Record<string, string>>({});
  const [answers, setAnswers] = useState<Record<string, string>>({});
  const [violations, setViolations] = useState<Violation[]>([]);
  const [picks, setPicks] = useState<Record<string, string>>({});
  const [justifications, setJustifications] = useState<Record<string, string>>({});

  const reasonsOf = (group: ReportGroup) => [...new Set(group.reports.map((r) => r.reason))];
  // The type a removal would be classified as, and whether it needs a
  // justification: another reason than any report's, or no strike at all.
  const pickFor = (group: ReportGroup) =>
    picks[keyOf(group)] ?? violations.find((v) => v.reason === group.reports[0].reason)?.id ?? "none";
  const needsJustification = (group: ReportGroup) => {
    const pick = pickFor(group);
    if (pick === "none") return true;
    const reason = violations.find((v) => v.id === pick)?.reason;
    return !reason || !reasonsOf(group).includes(reason);
  };
  // A live account is decided on the standing page (a measure, or parts of
  // its profile removed); everything else here.
  const liveAccount = (group: ReportGroup) => group.target_type === "user" && group.target_exists;
  const classifies = (group: ReportGroup) => !liveAccount(group) && group.target_type !== "user" && violations.length > 0;

  useEffect(() => {
    if (!authLoading && !user) router.push("/login");
  }, [user, authLoading, router]);

  useEffect(() => {
    if (!user) return;
    adminStandingApi.violations().then(setViolations).catch(() => {});
    adminReportsApi.list()
      .then(setGroups)
      .catch((err) => setError(err instanceof Error ? err.message : "Failed to load reports"))
      .finally(() => setLoading(false));
  }, [user]);

  const reload = () => adminReportsApi.list().then(setGroups).catch(() => {});

  const handleDismiss = async (group: ReportGroup, onlyThis?: QueueReport) => {
    const key = keyOf(group);
    if (group.objections.length > 0 && !onlyThis && !answers[key]?.trim()) {
      setError("Answer the objection first: dismissing accepts it, and the author reads your answer.");
      return;
    }
    setBusy(key);
    setError(null);
    try {
      await adminReportsApi.dismiss((onlyThis ?? group.reports[0]).id, notes[key], !!onlyThis, answers[key]);
      await reload();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to dismiss");
    } finally {
      setBusy(null);
    }
  };

  const handleRemove = async (group: ReportGroup) => {
    const prompt = !group.target_exists
      ? "Confirm this violation? It's decided on the preserved copy, and the author gets the strike."
      : group.target_type === "message"
        ? "Delete this message for both sides?"
        : "Remove this content? It disappears for everyone now and is deleted for good once the objection window has passed (child sexual abuse material: once its evidence copy exists).";
    if (!window.confirm(prompt)) return;
    const key = keyOf(group);
    setBusy(key);
    setError(null);
    try {
      await adminReportsApi.remove(
        group.reports[0].id,
        notes[key],
        classifies(group) ? pickFor(group) : undefined,
        classifies(group) ? justifications[key] : undefined,
      );
      await reload();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to remove");
    } finally {
      setBusy(null);
    }
  };

  if (authLoading || !user) return null;

  return (
    <div className="flex-1 bg-background">
      <header className="sticky top-0 z-10 flex h-14 items-center gap-3 overflow-x-auto border-b border-border bg-background/80 px-4 backdrop-blur">
        <SmartBackButton aria-label="Back" />
        <span className="flex-1 font-semibold">Reports</span>
        <Link href="/admin/cases/new" className="flex shrink-0 items-center gap-1 text-sm text-muted-foreground hover:text-foreground">
          <Plus size={16} /> Case
        </Link>
        <Link href="/admin/rights" className="flex shrink-0 items-center gap-1 text-sm text-muted-foreground hover:text-foreground">
          <FileWarning size={16} /> Claims
        </Link>
        <Link href="/admin/moderation" className="flex shrink-0 items-center gap-1 text-sm text-muted-foreground hover:text-foreground">
          <Scale size={16} /> Objections
        </Link>
        <Link href="/admin/decisions" className="flex shrink-0 items-center gap-1 text-sm text-muted-foreground hover:text-foreground">
          <History size={16} /> Log
        </Link>
        <Link href="/admin/standing" className="flex shrink-0 items-center gap-1 text-sm text-muted-foreground hover:text-foreground">
          <Gauge size={16} /> Standing
        </Link>
        <Link href="/admin/evidence" className="flex shrink-0 items-center gap-1 text-sm text-muted-foreground hover:text-foreground">
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

        {!loading && groups.length === 0 && (
          <div className="py-16 text-center">
            <ShieldAlert size={32} className="mx-auto mb-3 text-muted-foreground" />
            <p className="text-sm text-muted-foreground">No pending reports</p>
          </div>
        )}

        <div className="space-y-3">
          {groups.map((group) => {
            const key = keyOf(group);
            const isBusy = busy === key;
            const status = group.target_status ? STATUS_LABELS[group.target_status] : null;
            const accountReports = group.reports.filter((r) => r.source !== "authority_order").map((r) => r.id);
            return (
              <div key={key} data-testid="report-group" className="rounded-xl border border-border p-3">
                <div className="mb-2 flex flex-wrap items-center gap-2">
                  <SeverityBadge severity={group.severity} />
                  {group.overdue && (
                    <span className="flex items-center gap-1 rounded bg-destructive/15 px-1.5 py-0.5 text-xs font-semibold text-destructive">
                      <Clock size={11} /> 30+ days
                    </span>
                  )}
                  <span className="text-sm font-medium">{TARGET_LABELS[group.target_type]}</span>
                  {group.target_username && (
                    <span className="text-sm text-muted-foreground">
                      by{" "}
                      <Link href={`/users/${group.target_username}`} className="underline">
                        {group.target_username}
                      </Link>
                    </span>
                  )}
                  {status && <span className="rounded bg-muted px-1.5 py-0.5 text-xs">{status}</span>}
                  {!group.target_exists && (
                    <span className="rounded bg-muted px-1.5 py-0.5 text-xs">Deleted by its author</span>
                  )}
                  <span className="ml-auto text-xs text-muted-foreground">
                    {group.reports.length} {group.reports.length === 1 ? "report" : "reports"}
                  </span>
                </div>

                {(group.target_preview || group.target_thumb_url) && (
                  <div className="mb-2 flex items-start gap-2 rounded-md bg-muted/50 p-2">
                    {group.target_thumb_url && (
                      <Image
                        src={getMediaUrl(group.target_thumb_url)}
                        alt=""
                        width={56}
                        height={56}
                        className="h-14 w-14 shrink-0 rounded object-cover"
                        unoptimized
                      />
                    )}
                    {group.target_preview && <p className="line-clamp-3 text-sm">{group.target_preview}</p>}
                  </div>
                )}

                {/* Likely-illegal reports and every message report keep an
                    evidence copy. A message is only ever read there, and
                    CSAM and intimate images are never shown in this list:
                    opening the copy takes a reason and is logged. */}
                {group.evidence_id && (
                  <Link
                    href={`/admin/evidence/${group.evidence_id}`}
                    className="mb-2 flex items-center gap-1.5 rounded-md bg-muted/50 p-2 text-sm underline-offset-2 hover:underline"
                  >
                    <Archive size={14} />
                    {!group.target_exists || group.evidence_content_deleted
                      ? "Deleted, preserved as evidence — review it there"
                      : group.target_type === "message"
                        ? "Open the message and the ten before it (logged)"
                        : "Evidence copy (as reported, with any edits since)"}
                  </Link>
                )}

                <ul className="mb-2 space-y-2">
                  {group.reports.map((report) => (
                    <li key={report.id} className="rounded-md bg-muted/30 p-2 text-sm">
                      <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
                        <span className="font-medium">{REASON_LABELS[report.reason] ?? report.reason}</span>
                        {report.source !== "user_report" && (
                          <span className="rounded bg-primary/10 px-1.5 py-0.5 text-xs">{SOURCE_LABELS[report.source]}</span>
                        )}
                        {report.recheck_requested_at && (
                          <span className="flex items-center gap-1 rounded bg-amber-500/15 px-1.5 py-0.5 text-xs text-amber-700">
                            <RotateCcw size={11} /> Re-check requested
                          </span>
                        )}
                        <span className="ml-auto text-xs text-muted-foreground">
                          {new Date(report.created_at).toLocaleString()}
                        </span>
                      </div>
                      <p className="text-xs text-muted-foreground">
                        <Reporter report={report} />
                      </p>
                      {report.details && <p className="mt-1 italic">&ldquo;{report.details}&rdquo;</p>}
                      {report.recheck_note && (
                        <p className="mt-1 text-xs">Asked to look again: &ldquo;{report.recheck_note}&rdquo;</p>
                      )}
                      {group.reports.length > 1 && (
                        <button
                          type="button"
                          onClick={() => handleDismiss(group, report)}
                          disabled={isBusy}
                          className="mt-1 text-xs text-muted-foreground underline hover:text-foreground"
                        >
                          Dismiss only this report
                        </button>
                      )}
                    </li>
                  ))}
                </ul>

                {/* Objections against an automatic hide or warning that rests
                    on these reports: dismissing accepts them with the answer
                    below, removing replaces the restriction. */}
                {group.objections.map((objection) => (
                  <div key={objection.decision_id} className="mb-2 rounded-md border border-amber-500/40 p-2 text-sm">
                    <p className="text-xs font-medium text-amber-700">
                      The author objected to the automatic {objection.restriction === "hidden" ? "hide" : "warning"}
                    </p>
                    {objection.objection && <p className="mt-1 italic">&ldquo;{objection.objection}&rdquo;</p>}
                  </div>
                ))}
                {group.objections.length > 0 && (
                  <textarea
                    value={answers[key] ?? ""}
                    onChange={(e) => setAnswers((prev) => ({ ...prev, [key]: e.target.value }))}
                    placeholder="Answer to the objection if you dismiss (shown to the author)"
                    maxLength={1000}
                    rows={2}
                    disabled={isBusy}
                    className="mb-2 w-full rounded-md border border-input bg-transparent px-2 py-1.5 text-sm outline-none placeholder:text-muted-foreground focus:border-ring focus:ring-1 focus:ring-ring"
                  />
                )}

                {/* The author's recent activity on one page (logged), e.g. to
                    tell a hijacked account from a bot. */}
                {group.target_username && (
                  <p className="mb-2 text-xs">
                    <Link
                      href={`/admin/review/${group.target_username}?report=${group.reports[0].id}&reason=${encodeURIComponent(
                        `Report: ${REASON_LABELS[group.reports[0].reason] ?? group.reports[0].reason}`,
                      )}`}
                      className="font-medium underline"
                    >
                      Review account
                    </Link>
                    {group.reports.some((r) => ACCOUNT_REVIEW_REASONS.has(r.reason)) && (
                      <span className="text-muted-foreground"> — check whether it was taken over or is a bot</span>
                    )}
                  </p>
                )}

                <input
                  value={notes[key] ?? ""}
                  onChange={(e) => setNotes((prev) => ({ ...prev, [key]: e.target.value }))}
                  placeholder="Add a note for your records (optional, internal)"
                  maxLength={1000}
                  className="mb-2 w-full rounded-md border border-input bg-transparent px-2 py-1.5 text-sm outline-none placeholder:text-muted-foreground focus:border-ring focus:ring-1 focus:ring-ring"
                  disabled={isBusy}
                />

                {/* What the content was, if it is removed: a type from the
                    catalog, each with a written criterion and fixed points.
                    Content its author already deleted is classified too:
                    the decision rests on its preserved copy. Departing from
                    every report's reason, or no strike, needs a
                    justification. */}
                {classifies(group) && (() => {
                  const pick = pickFor(group);
                  const chosen = violations.find((v) => v.id === pick);
                  const own = reasonsOf(group);
                  const reasons = [...own, ...new Set(violations.map((v) => v.reason).filter((r) => !own.includes(r)))];
                  return (
                    <div className="mb-2 space-y-1.5 text-sm">
                      <label className="flex flex-wrap items-center gap-2">
                        <span className="text-muted-foreground">If removed, classify as:</span>
                        <select
                          value={pick}
                          onChange={(e) => setPicks((prev) => ({ ...prev, [key]: e.target.value }))}
                          disabled={isBusy}
                          className="min-w-0 max-w-full rounded-md border border-input bg-background px-2 py-1 text-sm"
                        >
                          {reasons.map((reason) => (
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
                      {needsJustification(group) && (
                        <input
                          value={justifications[key] ?? ""}
                          onChange={(e) => setJustifications((prev) => ({ ...prev, [key]: e.target.value }))}
                          placeholder={
                            pick === "none"
                              ? "Why no strike? (required, internal)"
                              : "Why another reason than the reports'? (required, internal)"
                          }
                          maxLength={1000}
                          disabled={isBusy}
                          aria-label="Justification"
                          className="w-full rounded-md border border-input bg-transparent px-2 py-1.5 text-sm outline-none placeholder:text-muted-foreground focus:border-ring focus:ring-1 focus:ring-ring"
                        />
                      )}
                    </div>
                  );
                })()}

                <div className="flex flex-wrap gap-2">
                  <Button size="sm" variant="outline" onClick={() => handleDismiss(group)} disabled={isBusy}>
                    <X size={14} className="mr-1" /> {group.reports.length > 1 ? "Dismiss all" : "Dismiss"}
                  </Button>
                  {liveAccount(group) ? (
                    // An account is decided on its standing page: a measure,
                    // or parts of the profile removed, with these reports.
                    <Link
                      href={`/admin/standing/${group.target_username}?reports=${accountReports.join(",")}`}
                      className="inline-flex h-8 items-center rounded-md bg-destructive px-3 text-sm font-medium text-white"
                    >
                      <Gauge size={14} className="mr-1" /> Decide on the account
                    </Link>
                  ) : (
                    <Button
                      size="sm"
                      variant="destructive"
                      onClick={() => handleRemove(group)}
                      disabled={isBusy || (classifies(group) && needsJustification(group) && !justifications[key]?.trim())}
                    >
                      {!group.target_exists
                        ? <><ShieldAlert size={14} className="mr-1" /> Confirm violation</>
                        : <><Trash2 size={14} className="mr-1" /> {group.target_type === "message" ? "Delete message" : "Remove content"}</>}
                    </Button>
                  )}
                </div>
              </div>
            );
          })}
        </div>
      </main>
    </div>
  );
}
