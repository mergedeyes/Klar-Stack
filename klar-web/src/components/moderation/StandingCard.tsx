"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import { Ban, UserX } from "lucide-react";
import {
  adminStandingApi,
  type AccountMeasure,
  type AccountMeasureRecord,
  type AdminStanding,
  type ProfileField,
  type ReportReason,
  type Strike,
  type StrikeDetail,
  type Violation,
} from "@/lib/api";
import { Button } from "@/components/ui/button";
import { StandingScore } from "@/components/moderation/StandingScore";
import { MEASURE_LABELS, REASON_LABELS, SEVERITY_LABELS, SOURCE_LABELS } from "@/lib/moderation";

// One account's standing for admins: score, strikes, earlier measures, and
// the two ways to act on the account itself -- a measure (warning or
// suspension), or removing parts of the profile. Used by the standing list
// and by the page for one account, which the report queue links to with
// the reports to close.

const inputClass =
  "w-full rounded-md border border-input bg-transparent px-2 py-1.5 text-sm outline-none placeholder:text-muted-foreground focus:border-ring focus:ring-1 focus:ring-ring";

const MEASURES: AccountMeasure[] = ["warning", "suspend_7d", "suspend_30d", "ban"];
const REASONS = Object.keys(REASON_LABELS) as ReportReason[];

const RECORD_LABELS: Record<AccountMeasureRecord["restriction"], string> = {
  warning: "Warning",
  suspended: "Suspended",
  banned: "Suspended permanently",
};

const when = (iso: string) => new Date(iso).toLocaleString();

// One strike, with the removed content, its context and the reports behind
// it on demand. Opening it is logged server-side (who, when), so it's a
// deliberate click rather than loaded with the list.
function StrikeRow({ strike, onError }: { strike: Strike; onError: (msg: string) => void }) {
  const [detail, setDetail] = useState<StrikeDetail | null>(null);
  const [loading, setLoading] = useState(false);

  const open = async () => {
    setLoading(true);
    try {
      setDetail(await adminStandingApi.openStrike(strike.id));
    } catch (err) {
      onError(err instanceof Error ? err.message : "Failed to open the strike");
    } finally {
      setLoading(false);
    }
  };

  const snap = detail?.snapshot;
  return (
    <li className="text-sm">
      <span className="font-medium">+{strike.points}</span>
      {strike.points > strike.base_points && <span className="text-muted-foreground"> (repeat, ×1.5)</span>}{" "}
      {strike.violation_label || strike.violation} · {REASON_LABELS[strike.reason] ?? strike.reason} · {strike.target_type}
      {!detail && strike.content_excerpt && (
        <span className="block truncate text-muted-foreground">{strike.content_excerpt}</span>
      )}
      <span className="block text-xs text-muted-foreground">
        {new Date(strike.created_at).toLocaleDateString()} ·{" "}
        {strike.expires_at ? `expires ${new Date(strike.expires_at).toLocaleDateString()}` : "doesn't expire"}
        {!detail && (
          <>
            {" · "}
            <button type="button" onClick={open} disabled={loading} className="underline">
              Show content
            </button>
          </>
        )}
      </span>

      {detail && (
        <div className="mt-1.5 space-y-2 rounded-md bg-muted/40 p-2">
          {snap?.context?.post && (
            <div className="text-xs text-muted-foreground">
              On a post by @{snap.context.post.author} ({when(snap.context.post.created_at)}):
              <p className="line-clamp-3 whitespace-pre-wrap">{snap.context.post.text || "(no caption)"}</p>
            </div>
          )}
          {snap?.context?.parent_comment && (
            <div className="text-xs text-muted-foreground">
              Replying to @{snap.context.parent_comment.author} ({when(snap.context.parent_comment.created_at)}):
              <p className="whitespace-pre-wrap">{snap.context.parent_comment.text}</p>
            </div>
          )}
          {snap?.content ? (
            <div>
              <p className="text-xs text-muted-foreground">
                The removed {snap.content.type} · written {when(snap.content.created_at)}
                {snap.content.edited_at && ` · edited ${when(snap.content.edited_at)}`}
                {!!snap.content.image_count && ` · ${snap.content.image_count} image(s), not kept here`}
              </p>
              <p className="whitespace-pre-wrap">{snap.content.text || "(no text)"}</p>
            </div>
          ) : (
            <p className="text-xs text-muted-foreground">The content was already gone when it was removed.</p>
          )}
          {snap && snap.reports.length > 0 && (
            <ul className="text-xs text-muted-foreground">
              {snap.reports.map((r, i) => (
                <li key={i}>
                  Reported {when(r.created_at)} · {REASON_LABELS[r.reason] ?? r.reason}
                  {r.details && <> · &ldquo;{r.details}&rdquo;</>}
                </li>
              ))}
            </ul>
          )}
          <p className="text-xs text-muted-foreground">
            Removed {snap && when(snap.removed_at)} by {detail.decided_by ?? "a deleted account"}
            {detail.criterion_de && <> · Criterion: {detail.criterion_de}</>}
          </p>
          {detail.reported_reason && (
            <p className="text-xs">
              Reported as {REASON_LABELS[detail.reported_reason] ?? detail.reported_reason}, classified differently:{" "}
              <em>{detail.justification}</em>
            </p>
          )}
          {!detail.reported_reason && detail.justification && (
            <p className="text-xs">Justification: <em>{detail.justification}</em></p>
          )}
          {detail.evidence_id && (
            <Link href={`/admin/evidence/${detail.evidence_id}`} className="block text-xs underline">
              Preserved copy with images (evidence, access logged)
            </Link>
          )}
        </div>
      )}
    </li>
  );
}

// The reason most of the account's strikes share, as the default ground
// for a measure.
function mainReason(s: AdminStanding): ReportReason {
  const counts = new Map<ReportReason, number>();
  for (const strike of s.strikes) counts.set(strike.reason, (counts.get(strike.reason) ?? 0) + strike.points);
  return [...counts.entries()].sort((a, b) => b[1] - a[1])[0]?.[0] ?? "harassment";
}

export function StandingCard({
  initial,
  onError,
  reportIds = [],
}: {
  initial: AdminStanding;
  onError: (msg: string) => void;
  // Pending reports on the account that a measure or a removal answers.
  reportIds?: string[];
}) {
  const [s, setS] = useState(initial);
  const [measure, setMeasure] = useState<AccountMeasure>(initial.suggestion ?? "warning");
  const [reason, setReason] = useState<ReportReason>(mainReason(initial));
  const [explanation, setExplanation] = useState("");
  // The reports a measure answers: the ones the queue sent along, or else
  // every pending report on the account, so a measure decided after
  // opening this page some other way still rests on them (and its
  // statement doesn't call it the team's own initiative). Authority orders
  // aren't ticked by default, like in the queue. Answered once; after that,
  // a further measure stands on its own.
  const [linked, setLinked] = useState(
    reportIds.length > 0
      ? reportIds
      : initial.pending_reports.filter((r) => r.source !== "authority_order").map((r) => r.id),
  );
  const toggleLinked = (id: string, on: boolean) =>
    setLinked((ids) => (on ? [...ids, id] : ids.filter((x) => x !== id)));
  const [busy, setBusy] = useState(false);

  // The score explains a measure only when there are strikes and the
  // measure doesn't go beyond the suggestion; otherwise the statement needs
  // the admin's own explanation (DSA Art. 17(3)(c): the facts relied on).
  const beyondSuggestion = !s.suggestion || MEASURES.indexOf(measure) > MEASURES.indexOf(s.suggestion);
  const needsExplanation = s.strikes.length === 0 || beyondSuggestion;

  const apply = async () => {
    if (!window.confirm(`${MEASURE_LABELS[measure]} for @${s.username}? They get a statement of reasons and can object.`)) return;
    setBusy(true);
    try {
      setS(await adminStandingApi.apply(s.username, measure, reason, explanation, linked));
      setExplanation("");
      setLinked([]);
    } catch (err) {
      onError(err instanceof Error ? err.message : "Failed to apply the measure");
    } finally {
      setBusy(false);
    }
  };

  const lift = async () => {
    if (!window.confirm(`Lift the suspension of @${s.username} now?`)) return;
    setBusy(true);
    try {
      await adminStandingApi.lift(s.username);
      setS(await adminStandingApi.get(s.username));
    } catch (err) {
      onError(err instanceof Error ? err.message : "Failed to lift the suspension");
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="rounded-xl border border-border p-3">
      <div className="mb-2 flex items-center justify-between gap-2">
        <span>
          <Link href={`/users/${s.username}`} className="font-medium hover:underline">
            @{s.username}
          </Link>{" "}
          <Link href={`/admin/review/${s.username}`} className="text-xs text-muted-foreground underline">
            Review
          </Link>
        </span>
        {s.suggestion && (
          <span className="rounded bg-amber-500/15 px-1.5 py-0.5 text-xs font-semibold text-amber-600">
            Suggested: {MEASURE_LABELS[s.suggestion]}
          </span>
        )}
      </div>

      {s.suspension && (
        <p className="mb-2 flex items-center gap-1.5 text-sm text-destructive">
          <Ban size={14} />
          {s.suspension.permanent || !s.suspension.until
            ? `Suspended permanently${s.suspension.deletion_at
                ? ` · account deleted on ${new Date(s.suspension.deletion_at).toLocaleDateString()}`
                : s.suspension.statement_held
                  ? " · deletion blocked: the statement hasn't been sent yet (Objections page)"
                  : " · deletion waits for the objection"}`
            : `Suspended until ${new Date(s.suspension.until).toLocaleString()}`}
        </p>
      )}

      {s.pending_reports.length > 0 && (
        <fieldset className="mb-2 rounded-md bg-primary/10 px-2 py-1 text-xs">
          <legend className="sr-only">Pending reports on this account</legend>
          <p className="mb-1">
            Pending reports on this account. What you decide here answers the ticked ones; the reporters are told.
          </p>
          {s.pending_reports.map((r) => (
            <label key={r.id} className="flex items-center gap-2">
              <input
                type="checkbox"
                checked={linked.includes(r.id)}
                onChange={(e) => toggleLinked(r.id, e.target.checked)}
              />
              {REASON_LABELS[r.reason] ?? r.reason} · {SOURCE_LABELS[r.source] ?? r.source} ·{" "}
              {new Date(r.created_at).toLocaleDateString()}
            </label>
          ))}
          {linked.length === 0 && (
            <p className="mt-1 text-muted-foreground">
              None ticked: a measure counts as the team&apos;s own initiative, and its statement says so.
            </p>
          )}
        </fieldset>
      )}

      <StandingScore standing={s} />

      {s.strikes.length > 0 && (
        <details className="mt-3">
          <summary className="cursor-pointer text-sm text-muted-foreground">
            {s.strikes.length} active strike{s.strikes.length === 1 ? "" : "s"}
          </summary>
          <ul className="mt-2 space-y-1.5">
            {s.strikes.map((strike) => <StrikeRow key={strike.id} strike={strike} onError={onError} />)}
          </ul>
        </details>
      )}

      {s.measures.length > 0 && (
        <details className="mt-2">
          <summary className="cursor-pointer text-sm text-muted-foreground">
            {s.measures.length} earlier measure{s.measures.length === 1 ? "" : "s"}
          </summary>
          <ul className="mt-2 space-y-1">
            {s.measures.map((m) => (
              <li key={m.id} className="text-sm">
                {RECORD_LABELS[m.restriction]}
                {m.suspension_days && ` ${m.suspension_days} days`} · {REASON_LABELS[m.reason] ?? m.reason}
                {m.standing_score !== null && ` · at ${m.standing_score} pts`}
                <span className="block text-xs text-muted-foreground">
                  {new Date(m.created_at).toLocaleString()}
                  {m.lifted_at && " · lifted"}
                  {m.superseded && " · replaced"}
                  {!m.delivered && " · statement held back"}
                  {m.objection_status && ` · objection ${m.objection_status}`}
                </span>
              </li>
            ))}
          </ul>
        </details>
      )}

      {/* The suggestion is a guide; the admin weighs the case and may pick
          any measure (DSA Art. 23 asks for a case-by-case assessment). */}
      <textarea
        value={explanation}
        onChange={(e) => setExplanation(e.target.value)}
        placeholder={
          needsExplanation
            ? "Why this measure? Required: shown to the user in the statement"
            : "Anything to add? Optional: shown to the user in the statement"
        }
        aria-label="Explanation for the user"
        maxLength={1000}
        rows={2}
        disabled={busy}
        className={`${inputClass} mt-3`}
      />
      <div className="mt-2 flex flex-wrap items-center gap-2 border-t border-border pt-3">
        <select
          value={measure}
          onChange={(e) => setMeasure(e.target.value as AccountMeasure)}
          disabled={busy}
          aria-label="Measure"
          className="rounded-md border border-input bg-background px-2 py-1 text-sm"
        >
          {MEASURES.map((m) => (
            <option key={m} value={m}>{MEASURE_LABELS[m]}</option>
          ))}
        </select>
        <select
          value={reason}
          onChange={(e) => setReason(e.target.value as ReportReason)}
          disabled={busy}
          aria-label="Main reason"
          className="min-w-0 max-w-full rounded-md border border-input bg-background px-2 py-1 text-sm"
        >
          {REASONS.map((r) => (
            <option key={r} value={r}>{REASON_LABELS[r]}</option>
          ))}
        </select>
        <Button size="sm" variant="destructive" onClick={apply} disabled={busy || (needsExplanation && !explanation.trim())}>
          Apply
        </Button>
        {s.suspension && (
          <Button size="sm" variant="outline" onClick={lift} disabled={busy}>
            Lift suspension
          </Button>
        )}
      </div>

      <ProfileRemoval
        username={s.username}
        reportIds={linked}
        onDone={async () => {
          setLinked([]);
          setS(await adminStandingApi.get(s.username));
        }}
        onError={onError}
      />
    </div>
  );
}

const FIELD_LABELS: Record<ProfileField, string> = {
  avatar: "Profile picture",
  bio: "Bio",
  display_name: "Display name",
  username: "Username (replaced by user_…)",
};

// Removing parts of a profile instead of acting on the whole account: a
// decision like a content removal, classified from the catalog, with a
// strike and a statement; an accepted objection puts them back.
function ProfileRemoval({
  username,
  reportIds,
  onDone,
  onError,
}: {
  username: string;
  reportIds: string[];
  onDone: () => Promise<void>;
  onError: (msg: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const [fields, setFields] = useState<ProfileField[]>([]);
  const [violations, setViolations] = useState<Violation[]>([]);
  const [violation, setViolation] = useState("");
  const [justification, setJustification] = useState("");
  const [note, setNote] = useState("");
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (open && violations.length === 0) adminStandingApi.violations().then(setViolations).catch(() => {});
  }, [open, violations.length]);

  const toggle = (field: ProfileField) =>
    setFields((prev) => (prev.includes(field) ? prev.filter((f) => f !== field) : [...prev, field]));

  const submit = async () => {
    if (!window.confirm(`Remove ${fields.map((f) => FIELD_LABELS[f].toLowerCase()).join(", ")} from @${username}'s profile?`)) {
      return;
    }
    setBusy(true);
    try {
      await adminStandingApi.removeFromProfile(username, fields, violation || undefined, justification, note, reportIds);
      setFields([]);
      setJustification("");
      setNote("");
      setOpen(false);
      await onDone();
    } catch (err) {
      onError(err instanceof Error ? err.message : "Failed to remove");
    } finally {
      setBusy(false);
    }
  };

  if (!open) {
    return (
      <button type="button" onClick={() => setOpen(true)} className="mt-3 flex items-center gap-1 text-sm underline">
        <UserX size={14} /> Remove parts of the profile
      </button>
    );
  }

  const reasons = [...new Set(violations.map((v) => v.reason))];
  return (
    <div className="mt-3 space-y-2 border-t border-border pt-3 text-sm">
      <p className="font-medium">Remove from the profile</p>
      <div className="flex flex-wrap gap-x-4 gap-y-1">
        {(Object.keys(FIELD_LABELS) as ProfileField[]).map((field) => (
          <label key={field} className="flex items-center gap-1.5">
            <input type="checkbox" checked={fields.includes(field)} onChange={() => toggle(field)} disabled={busy} />
            {FIELD_LABELS[field]}
          </label>
        ))}
      </div>
      <select
        value={violation}
        onChange={(e) => setViolation(e.target.value)}
        disabled={busy}
        aria-label="Classify as"
        className="min-w-0 max-w-full rounded-md border border-input bg-background px-2 py-1 text-sm"
      >
        <option value="">{reportIds.length > 0 ? "As reported (the report's first type)" : "Classify as…"}</option>
        {reasons.map((reason) => (
          <optgroup key={reason} label={REASON_LABELS[reason] ?? reason}>
            {violations.filter((v) => v.reason === reason).map((v) => (
              <option key={v.id} value={v.id}>{v.label} · {SEVERITY_LABELS[v.severity]}</option>
            ))}
          </optgroup>
        ))}
        <option value="none">No strike (needs a justification)</option>
      </select>
      <input
        value={justification}
        onChange={(e) => setJustification(e.target.value)}
        placeholder="Justification (internal; required for another reason than the report's, or no strike)"
        maxLength={1000}
        disabled={busy}
        className={inputClass}
      />
      <input
        value={note}
        onChange={(e) => setNote(e.target.value)}
        placeholder="Note for your records (optional, internal)"
        maxLength={1000}
        disabled={busy}
        className={inputClass}
      />
      <div className="flex gap-2">
        <Button
          size="sm"
          variant="destructive"
          onClick={submit}
          disabled={busy || fields.length === 0 || (reportIds.length === 0 && !violation)}
        >
          Remove
        </Button>
        <Button size="sm" variant="outline" onClick={() => setOpen(false)} disabled={busy}>
          Cancel
        </Button>
      </div>
    </div>
  );
}

