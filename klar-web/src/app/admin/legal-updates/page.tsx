"use client";

import { useEffect, useMemo, useState } from "react";
import { useRouter } from "next/navigation";
import { FileText } from "lucide-react";
import { useAuth } from "@/lib/auth-context";
import { adminLegalUpdatesApi, type AdminLegalUpdate, type LegalDocument } from "@/lib/api";
import { SmartBackButton } from "@/components/SmartBackButton";

const DOCUMENT_LABELS: Record<LegalDocument, string> = {
  terms: "Nutzungsbedingungen",
  privacy: "Datenschutzerklärung",
};

type Sort = "latest" | "oldest" | "most_accepted" | "least_accepted" | "most_emails" | "least_emails";

const SORT_LABELS: Record<Sort, string> = {
  latest: "Latest published",
  oldest: "Oldest published",
  most_accepted: "Most accepted",
  least_accepted: "Least accepted",
  most_emails: "Most emails sent",
  least_emails: "Least emails sent",
};

// Accepted (or seen) as a share of the accounts that existed when it was
// published: counts aren't comparable, since later notices reach more
// accounts.
const acceptedShare = (u: AdminLegalUpdate) => (u.audience > 0 ? u.acknowledged / u.audience : 0);

const COMPARE: Record<Sort, (a: AdminLegalUpdate, b: AdminLegalUpdate) => number> = {
  latest: (a, b) => b.published_at.localeCompare(a.published_at),
  oldest: (a, b) => a.published_at.localeCompare(b.published_at),
  most_accepted: (a, b) => acceptedShare(b) - acceptedShare(a) || b.acknowledged - a.acknowledged,
  least_accepted: (a, b) => acceptedShare(a) - acceptedShare(b) || a.acknowledged - b.acknowledged,
  most_emails: (a, b) => b.emails_sent - a.emails_sent,
  least_emails: (a, b) => a.emails_sent - b.emails_sent,
};

const inputClass = "rounded-md border border-input bg-background px-2 py-1 text-sm";

// Notices about changed Terms or privacy policy, with their progress. They
// aren't written here: each is a file in klar-web/legal-updates, added in
// the pull request that changes the page (CI refuses the change without
// one), and the frontend deploy publishes it once the new page is live.
export default function AdminLegalUpdatesPage() {
  const { user, loading: authLoading } = useAuth();
  const router = useRouter();

  const [updates, setUpdates] = useState<AdminLegalUpdate[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  // Published within [from, to] (dates in the admin's time zone, both
  // days included), in the chosen order.
  const [from, setFrom] = useState("");
  const [to, setTo] = useState("");
  const [sort, setSort] = useState<Sort>("latest");

  const shown = useMemo(() => {
    if (!updates) return null;
    const start = from ? new Date(`${from}T00:00:00`).getTime() : -Infinity;
    const end = to ? new Date(`${to}T23:59:59.999`).getTime() : Infinity;
    return updates
      .filter((u) => {
        const t = new Date(u.published_at).getTime();
        return t >= start && t <= end;
      })
      .sort(COMPARE[sort]);
  }, [updates, from, to, sort]);

  useEffect(() => {
    if (!authLoading && !user) router.push("/login");
  }, [user, authLoading, router]);

  useEffect(() => {
    if (!user) return;
    adminLegalUpdatesApi.list()
      .then(setUpdates)
      .catch((err) => setError(err instanceof Error ? err.message : "Failed to load"));
  }, [user]);

  if (authLoading || !user) return null;

  return (
    <div className="flex-1 bg-background">
      <header className="sticky top-0 z-10 flex h-14 items-center gap-3 border-b border-border bg-background/80 px-4 backdrop-blur">
        <SmartBackButton aria-label="Back" />
        <span className="font-semibold">Legal updates</span>
      </header>

      <main className="mx-auto max-w-2xl px-4 py-4">
        {error && <div className="mb-4 rounded-md bg-destructive/10 px-3 py-2 text-sm text-destructive">{error}</div>}

        <div className="mb-6 space-y-2 rounded-xl border border-border p-3 text-sm">
          <h2 className="flex items-center gap-1.5 font-semibold">
            <FileText size={14} /> How notices are published
          </h2>
          <p className="text-muted-foreground">
            In the pull request that changes the Terms or the privacy page, add a file to{" "}
            <code className="rounded bg-muted px-1">klar-web/legal-updates/</code>: a line{" "}
            <code className="rounded bg-muted px-1">documents: terms, privacy</code>, a line{" "}
            <code className="rounded bg-muted px-1">---</code>, then a short summary in plain German. CI refuses the
            change without one, unless the PR has the label{" "}
            <code className="rounded bg-muted px-1">legal: no notice</code> (e.g. a typo fix).
          </p>
          <p className="text-muted-foreground">
            After the merge, the deploy waits until the new page is live and publishes each file once: every existing
            account sees it on its next visit (Terms changes must be accepted), verified addresses get an email.
          </p>
        </div>

        <h2 className="mb-2 text-sm font-semibold">Published</h2>
        {updates?.length === 0 && <p className="text-sm text-muted-foreground">Nothing published yet.</p>}
        {updates && updates.length > 0 && (
          <div className="mb-3 flex flex-wrap items-end gap-2 text-sm">
            <label className="flex flex-col gap-1">
              <span className="text-xs text-muted-foreground">From</span>
              <input type="date" value={from} max={to || undefined} onChange={(e) => setFrom(e.target.value)} className={inputClass} />
            </label>
            <label className="flex flex-col gap-1">
              <span className="text-xs text-muted-foreground">To</span>
              <input type="date" value={to} min={from || undefined} onChange={(e) => setTo(e.target.value)} className={inputClass} />
            </label>
            <label className="flex flex-col gap-1">
              <span className="text-xs text-muted-foreground">Sort by</span>
              <select value={sort} onChange={(e) => setSort(e.target.value as Sort)} className={inputClass}>
                {(Object.keys(SORT_LABELS) as Sort[]).map((s) => (
                  <option key={s} value={s}>{SORT_LABELS[s]}</option>
                ))}
              </select>
            </label>
            {(from || to || sort !== "latest") && (
              <button
                type="button"
                onClick={() => { setFrom(""); setTo(""); setSort("latest"); }}
                className="pb-1.5 text-xs underline"
              >
                Reset
              </button>
            )}
            <span className="ml-auto pb-1.5 text-xs text-muted-foreground" role="status">
              {shown?.length} of {updates.length}
            </span>
          </div>
        )}
        {shown?.length === 0 && updates && updates.length > 0 && (
          <p className="text-sm text-muted-foreground">No notice published in this date range.</p>
        )}
        <div className="space-y-3">
          {shown?.map((u) => (
            <div key={u.id} className="rounded-xl border border-border p-3 text-sm">
              <p className="font-medium">
                {u.documents.map((d) => DOCUMENT_LABELS[d]).join(" + ")}
                {u.requires_acceptance && <span className="font-normal text-muted-foreground"> · must be accepted</span>}
              </p>
              <p className="text-xs text-muted-foreground">
                {new Date(u.published_at).toLocaleString()} · {u.source_key ?? "published by hand"} ·{" "}
                {u.acknowledged} of {u.audience} accounts ({Math.round(acceptedShare(u) * 100)} %){" "}
                {u.requires_acceptance ? "accepted" : "saw it"} ·{" "}
                {u.emails_sent} emails{u.emails_finished_at ? " (done)" : " (sending…)"}
              </p>
              <p className="mt-2 whitespace-pre-wrap">{u.summary}</p>
            </div>
          ))}
        </div>
      </main>
    </div>
  );
}
