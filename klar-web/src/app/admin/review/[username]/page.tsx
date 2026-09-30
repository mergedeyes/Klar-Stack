"use client";

import { Suspense, useEffect, useState } from "react";
import Link from "next/link";
import { useParams, useRouter, useSearchParams } from "next/navigation";
import { Bot, CheckCircle2, Lock, ScanSearch } from "lucide-react";
import { useAuth } from "@/lib/auth-context";
import { adminReviewApi, type AccountReview, type ReviewOutcome } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { SmartBackButton } from "@/components/SmartBackButton";
import { FLAG_LABELS, REASON_LABELS } from "@/lib/moderation";

const inputClass =
  "w-full rounded-md border border-input bg-transparent px-2 py-1.5 text-sm outline-none placeholder:text-muted-foreground focus:border-ring focus:ring-1 focus:ring-ring";

const when = (iso: string) => new Date(iso).toLocaleString();

function Section({ title, count, children, open = false }: {
  title: string;
  count?: number;
  children: React.ReactNode;
  open?: boolean;
}) {
  return (
    <details open={open} className="rounded-xl border border-border p-3">
      <summary className="cursor-pointer text-sm font-semibold">
        {title}
        {count !== undefined && <span className="font-normal text-muted-foreground"> ({count})</span>}
      </summary>
      <div className="mt-2 space-y-2 text-sm">{children}</div>
    </details>
  );
}

function Empty() {
  return <p className="text-muted-foreground">Nothing yet.</p>;
}

const OUTCOMES: { value: ReviewOutcome; label: string; icon: typeof Lock; help: string; confirm: string }[] = [
  {
    value: "no_action",
    label: "No action",
    icon: CheckCircle2,
    help: "The account looks fine.",
    confirm: "Close the review without action?",
  },
  {
    value: "lock",
    label: "Lock as possibly hacked",
    icon: Lock,
    help: "Signs it out everywhere and emails the owner a link to set a new password. Your note is the lock's reason.",
    confirm: "Lock this account and email the owner?",
  },
  {
    value: "bot",
    label: "Bot or spam-only account",
    icon: Bot,
    help: "Permanent suspension with a statement of reasons; the account is deleted after the objection window.",
    confirm: "Suspend this account permanently as a bot or spam-only account?",
  },
];

// One account's recent activity on one page, to decide whether it was taken
// over, is a bot, or is fine. Opening it is recorded (who, when, why), so it
// starts with a reason; the report it came from fills that in.
function ReviewPage() {
  const { user, loading: authLoading } = useAuth();
  const router = useRouter();
  const { username } = useParams<{ username: string }>();
  const searchParams = useSearchParams();
  const reportId = searchParams.get("report") ?? undefined;

  const [reason, setReason] = useState(searchParams.get("reason") ?? "");
  const [review, setReview] = useState<AccountReview | null>(null);
  const [note, setNote] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [decided, setDecided] = useState<string | null>(null);

  useEffect(() => {
    if (!authLoading && !user) router.push("/login");
  }, [user, authLoading, router]);

  const open = async (e: React.FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      setReview(await adminReviewApi.open(username, reason.trim(), reportId));
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to open the review");
    } finally {
      setBusy(false);
    }
  };

  const decide = async (outcome: (typeof OUTCOMES)[number]) => {
    if (!review || !window.confirm(outcome.confirm)) return;
    setBusy(true);
    setError(null);
    try {
      await adminReviewApi.decide(review.review_id, outcome.value, note.trim());
      setDecided(outcome.label);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to record the decision");
    } finally {
      setBusy(false);
    }
  };

  if (authLoading || !user) return null;

  return (
    <div className="flex-1 bg-background pb-12">
      <header className="sticky top-0 z-10 flex h-14 items-center gap-3 border-b border-border bg-background/80 px-4 backdrop-blur">
        <SmartBackButton aria-label="Back" />
        <span className="min-w-0 flex-1 truncate font-semibold">Review @{username}</span>
      </header>

      <main className="mx-auto max-w-2xl space-y-3 px-4 py-4">
        {error && <div className="rounded-md bg-destructive/10 px-3 py-2 text-sm text-destructive">{error}</div>}

        {!review && (
          <form onSubmit={open} className="space-y-2 rounded-xl border border-border p-3">
            <h2 className="flex items-center gap-1.5 text-sm font-semibold">
              <ScanSearch size={14} /> Why are you reviewing this account?
            </h2>
            <p className="text-xs text-muted-foreground">
              The review shows the account&apos;s recent posts, comments, likes and follows on one page, and message
              counts (never message content). Opening it is recorded with your reason.
            </p>
            <input
              value={reason}
              onChange={(e) => setReason(e.target.value)}
              placeholder="e.g. Spam report on a comment"
              aria-label="Reason"
              maxLength={2000}
              disabled={busy}
              className={inputClass}
            />
            <Button size="sm" type="submit" disabled={busy || !reason.trim()}>
              Open review
            </Button>
          </form>
        )}

        {review && (
          <>
            <div className="rounded-xl border border-border p-3 text-sm">
              <p>
                <Link href={`/users/${review.overview.username}`} className="font-medium hover:underline">
                  @{review.overview.username}
                </Link>
                {review.overview.display_name && <span className="text-muted-foreground"> · {review.overview.display_name}</span>}
              </p>
              <p className="text-xs text-muted-foreground">
                Joined {new Date(review.overview.created_at).toLocaleDateString()} ·{" "}
                {review.overview.email_verified ? "email verified" : "email not verified"}
                {review.overview.is_private && " · private"} · {review.overview.post_count} posts ·{" "}
                {review.overview.follower_count} followers · follows {review.overview.following_count} · standing{" "}
                {review.standing_score}/100
                {review.suspended && " · suspended"}
                {review.locked && " · locked"}
              </p>
              <div className="mt-2 flex flex-wrap gap-1.5">
                {review.flags.length === 0 && <span className="text-xs text-muted-foreground">No signals.</span>}
                {review.flags.map((f) => (
                  <span key={f} className="rounded bg-amber-500/15 px-1.5 py-0.5 text-xs font-semibold text-amber-600">
                    {FLAG_LABELS[f]}
                  </span>
                ))}
              </div>
            </div>

            <Section title="Signals (last 7 days)" open>
              <ul className="space-y-0.5">
                <li>Most posts, comments and messages in 10 minutes: <strong>{review.signals.max_burst}</strong></li>
                <li>Most posts or comments with the same text: <strong>{review.signals.max_duplicates}</strong></li>
                <li>Posts and comments with a link: <strong>{review.signals.links}</strong></li>
                <li>
                  Activity in the last 24 hours: <strong>{review.signals.activity_24h}</strong>
                  {review.signals.woke_up && " (after 60+ days of silence)"}
                </li>
              </ul>
            </Section>

            <Section title="Direct messages (numbers only)" open>
              <p>
                Last 24 hours: <strong>{review.messages.sent_24h}</strong> sent to{" "}
                <strong>{review.messages.recipients_24h}</strong> conversations · last 7 days:{" "}
                <strong>{review.messages.sent_7d}</strong> to <strong>{review.messages.recipients_7d}</strong>
              </p>
            </Section>

            <Section title="Posts" count={review.posts.length}>
              {review.posts.length === 0 && <Empty />}
              {review.posts.map((p) => (
                <div key={p.id}>
                  <Link href={`/posts/${p.id}`} className="line-clamp-3 whitespace-pre-wrap hover:underline">
                    {p.caption || "(no caption)"}
                  </Link>
                  <p className="text-xs text-muted-foreground">
                    {when(p.created_at)} · {p.image_count} image(s)
                    {p.moderation_status !== "visible" && ` · ${p.moderation_status}`}
                  </p>
                </div>
              ))}
            </Section>

            <Section title="Comments" count={review.comments.length}>
              {review.comments.length === 0 && <Empty />}
              {review.comments.map((c) => (
                <div key={c.id}>
                  <p className="whitespace-pre-wrap">{c.body}</p>
                  <p className="text-xs text-muted-foreground">
                    {when(c.created_at)} · on{" "}
                    <Link href={`/posts/${c.post_id}`} className="underline">
                      a post by @{c.post_author ?? "deleted"}
                    </Link>
                  </p>
                </div>
              ))}
            </Section>

            <Section title="Likes given" count={review.likes.length}>
              {review.likes.length === 0 && <Empty />}
              {review.likes.map((l) => (
                <p key={`${l.post_id}-${l.created_at}`} className="text-xs">
                  {when(l.created_at)} ·{" "}
                  <Link href={`/posts/${l.post_id}`} className="underline">
                    post by @{l.post_author ?? "deleted"}
                  </Link>
                </p>
              ))}
            </Section>

            <Section title="Recently followed" count={review.follows.length}>
              {review.follows.length === 0 && <Empty />}
              {review.follows.map((f) => (
                <p key={f.username} className="text-xs">
                  {when(f.created_at)} ·{" "}
                  <Link href={`/users/${f.username}`} className="underline">@{f.username}</Link>
                </p>
              ))}
            </Section>

            <Section title="Reports about this account" count={review.reports.length}>
              {review.reports.length === 0 && <Empty />}
              {review.reports.map((r, i) => (
                <div key={i} className="text-xs">
                  <p>
                    {REASON_LABELS[r.reason] ?? r.reason} · {r.target_type} · {r.status} · {when(r.created_at)}
                  </p>
                  {r.details && <p className="italic text-muted-foreground">&ldquo;{r.details}&rdquo;</p>}
                </div>
              ))}
            </Section>

            <Section title="History" count={review.history.length}>
              {review.history.length === 0 && <Empty />}
              {review.history.map((h, i) => (
                <p key={i} className="text-xs">
                  {when(h.at)} · {h.kind}
                  {h.what && `: ${h.what.replace(/_/g, " ")}`}
                </p>
              ))}
            </Section>

            {decided ? (
              <div className="rounded-md bg-muted px-3 py-2 text-sm" role="status">
                Decided: {decided}.
              </div>
            ) : (
              <div className="space-y-2 rounded-xl border border-border p-3">
                <h2 className="text-sm font-semibold">Decision</h2>
                <textarea
                  value={note}
                  onChange={(e) => setNote(e.target.value)}
                  placeholder="What did you find? (required, internal)"
                  aria-label="Note"
                  maxLength={2000}
                  rows={2}
                  disabled={busy}
                  className={inputClass}
                />
                <div className="space-y-2">
                  {OUTCOMES.map((o) => (
                    <div key={o.value} className="flex flex-wrap items-center gap-2">
                      <Button
                        size="sm"
                        variant={o.value === "no_action" ? "outline" : "destructive"}
                        onClick={() => decide(o)}
                        disabled={busy || !note.trim()}
                      >
                        <o.icon size={14} className="mr-1" /> {o.label}
                      </Button>
                      <span className="text-xs text-muted-foreground">{o.help}</span>
                    </div>
                  ))}
                </div>
              </div>
            )}
          </>
        )}
      </main>
    </div>
  );
}

export default function AccountReviewPage() {
  return (
    <Suspense fallback={null}>
      <ReviewPage />
    </Suspense>
  );
}
