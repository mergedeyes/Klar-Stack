"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { Ban, Gauge, Search } from "lucide-react";
import { useAuth } from "@/lib/auth-context";
import {
  adminStandingApi,
  type AccountMeasure,
  type AccountMeasureRecord,
  type AdminStanding,
  type ReportReason,
  type Strike,
  type StrikeDetail,
} from "@/lib/api";
import { Button } from "@/components/ui/button";
import { SmartBackButton } from "@/components/SmartBackButton";
import { StandingScore } from "@/components/moderation/StandingScore";
import { MEASURE_LABELS, REASON_LABELS } from "@/lib/moderation";

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

function StandingCard({ initial, onError }: { initial: AdminStanding; onError: (msg: string) => void }) {
  const [s, setS] = useState(initial);
  const [measure, setMeasure] = useState<AccountMeasure>(initial.suggestion ?? "warning");
  const [reason, setReason] = useState<ReportReason>(mainReason(initial));
  const [busy, setBusy] = useState(false);

  const apply = async () => {
    if (!window.confirm(`${MEASURE_LABELS[measure]} for @${s.username}? They get a statement of reasons and can object.`)) return;
    setBusy(true);
    try {
      setS(await adminStandingApi.apply(s.username, measure, reason));
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
        <Link href={`/users/${s.username}`} className="font-medium hover:underline">
          @{s.username}
        </Link>
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
                : " · deletion waits for the objection"}`
            : `Suspended until ${new Date(s.suspension.until).toLocaleString()}`}
        </p>
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
      <div className="mt-3 flex flex-wrap items-center gap-2 border-t border-border pt-3">
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
        <Button size="sm" variant="destructive" onClick={apply} disabled={busy}>
          Apply
        </Button>
        {s.suspension && (
          <Button size="sm" variant="outline" onClick={lift} disabled={busy}>
            Lift suspension
          </Button>
        )}
      </div>
    </div>
  );
}

// Accounts that reached the warning threshold or are suspended, plus a
// lookup for any other account.
export default function AdminStandingPage() {
  const { user, loading: authLoading } = useAuth();
  const router = useRouter();

  const [list, setList] = useState<AdminStanding[] | null>(null);
  const [lookup, setLookup] = useState("");
  const [looked, setLooked] = useState<AdminStanding | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!authLoading && !user) router.push("/login");
  }, [user, authLoading, router]);

  useEffect(() => {
    if (!user) return;
    adminStandingApi.list()
      .then(setList)
      .catch((err) => setError(err instanceof Error ? err.message : "Failed to load"));
  }, [user]);

  const find = async (e: React.FormEvent) => {
    e.preventDefault();
    const name = lookup.trim().replace(/^@/, "");
    if (!name) return;
    setError(null);
    setLooked(null);
    try {
      setLooked(await adminStandingApi.get(name));
    } catch (err) {
      setError(err instanceof Error ? err.message : "User not found");
    }
  };

  if (authLoading || !user) return null;

  return (
    <div className="flex-1 bg-background">
      <header className="sticky top-0 z-10 flex h-14 items-center gap-3 border-b border-border bg-background/80 px-4 backdrop-blur">
        <SmartBackButton aria-label="Back" />
        <span className="font-semibold">Account standing</span>
      </header>

      <main className="mx-auto max-w-2xl px-4 py-4">
        {error && (
          <div className="mb-4 rounded-md bg-destructive/10 px-3 py-2 text-sm text-destructive">{error}</div>
        )}

        <form onSubmit={find} className="mb-4 flex gap-2">
          <input
            value={lookup}
            onChange={(e) => setLookup(e.target.value)}
            placeholder="Look up an account by username"
            className="min-w-0 flex-1 rounded-md border border-input bg-transparent px-2 py-1.5 text-sm outline-none placeholder:text-muted-foreground focus:border-ring focus:ring-1 focus:ring-ring"
          />
          <Button size="sm" type="submit" variant="outline">
            <Search size={14} className="mr-1" /> Look up
          </Button>
        </form>

        {looked && (
          <div className="mb-6">
            <StandingCard key={looked.user_id} initial={looked} onError={setError} />
          </div>
        )}

        <h2 className="mb-2 text-sm font-semibold">Needs attention</h2>
        {list === null && !error && (
          <div className="py-16 text-center text-sm text-muted-foreground animate-pulse">Loading…</div>
        )}
        {list && list.length === 0 && (
          <div className="py-16 text-center">
            <Gauge size={32} className="mx-auto mb-3 text-muted-foreground" />
            <p className="text-sm text-muted-foreground">No account has reached the warning threshold</p>
          </div>
        )}
        <div className="space-y-3">
          {list?.map((s) => <StandingCard key={s.user_id} initial={s} onError={setError} />)}
        </div>
      </main>
    </div>
  );
}
