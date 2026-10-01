"use client";

import { Suspense, useEffect, useState } from "react";
import { useRouter, useSearchParams } from "next/navigation";
import { adminReportsApi, adminStandingApi, type ReportReason } from "@/lib/api";
import { useAuth } from "@/lib/auth-context";
import { Button } from "@/components/ui/button";
import { SmartBackButton } from "@/components/SmartBackButton";
import { REASON_LABELS } from "@/lib/moderation";

const inputClass =
  "w-full rounded-md border border-input bg-transparent px-2 py-1.5 text-sm outline-none placeholder:text-muted-foreground focus:border-ring focus:ring-1 focus:ring-ring";

const REASONS = (Object.keys(REASON_LABELS) as ReportReason[]).filter((r) => r !== "copyright");
const UUID = "[0-9a-fA-F-]{36}";

type Target = { type: "post" | "comment" | "user"; id: string };

// A link to a post, a comment (…#comment-<id>) or a profile, as the target
// of a case. A profile is looked up by its name.
async function targetFromLink(link: string): Promise<Target | null> {
  const comment = link.match(new RegExp(`/posts/${UUID}.*comment[-=](${UUID})`));
  if (comment) return { type: "comment", id: comment[1] };
  const post = link.match(new RegExp(`/posts/(${UUID})`));
  if (post) return { type: "post", id: post[1] };
  const profile = link.match(/\/users\/([A-Za-z0-9_.]+)/);
  if (profile) return { type: "user", id: (await adminStandingApi.get(profile[1])).user_id };
  return null;
}

// Opens a case without a report: something the team came across itself,
// or an authority's order to act against content (Art. 9 DSA; a removal
// order for terrorist content has to be carried out within one hour). The
// case goes into the report queue and is decided there; the statement says
// where it came from. The post, comment and profile menus link here with
// ?type=…&id=….
function NewCasePage() {
  const { user, loading: authLoading } = useAuth();
  const router = useRouter();
  const searchParams = useSearchParams();
  const given = searchParams.get("type") && searchParams.get("id")
    ? { type: searchParams.get("type") as Target["type"], id: searchParams.get("id")! }
    : null;

  const [link, setLink] = useState("");
  const [reason, setReason] = useState<ReportReason>("terrorism");
  const [details, setDetails] = useState("");
  const [source, setSource] = useState<"own_initiative" | "authority_order">("own_initiative");
  const [authority, setAuthority] = useState("");
  const [reference, setReference] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!authLoading && !user) router.push("/login");
  }, [user, authLoading, router]);

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      const target = given ?? (await targetFromLink(link.trim()));
      if (!target) throw new Error("Paste a link to a post, a comment or a profile on Klar.");
      await adminReportsApi.createCase({
        target_type: target.type,
        target_id: target.id,
        reason,
        details,
        source,
        authority,
        order_reference: reference,
      });
      router.push("/admin/reports");
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to open the case");
      setBusy(false);
    }
  };

  if (authLoading || !user) return null;

  return (
    <div className="flex-1 bg-background">
      <header className="sticky top-0 z-10 flex h-14 items-center gap-3 border-b border-border bg-background/80 px-4 backdrop-blur">
        <SmartBackButton aria-label="Back" />
        <span className="font-semibold">Open a case</span>
      </header>
      <main className="mx-auto max-w-2xl px-4 py-4">
        <p className="mb-4 text-sm text-muted-foreground">
          For content nobody reported: something you came across, or an authority&rsquo;s order. The case goes into the
          report queue and is decided there like any report. A removal order for terrorist content has to be carried out
          within one hour — remove it right after opening the case (see the admin runbook).
        </p>
        {error && <div className="mb-4 rounded-md bg-destructive/10 px-3 py-2 text-sm text-destructive">{error}</div>}
        <form onSubmit={submit} className="space-y-3 text-sm">
          {given ? (
            <p className="rounded-md bg-muted/50 px-2 py-1.5">
              {given.type === "user" ? "An account" : given.type === "post" ? "A post" : "A comment"} ({given.id})
            </p>
          ) : (
            <label className="block space-y-1">
              <span className="font-medium">Link</span>
              <input value={link} onChange={(e) => setLink(e.target.value)} required placeholder="https://www.klarsocial.eu/posts/…" className={inputClass} />
            </label>
          )}
          <label className="block space-y-1">
            <span className="font-medium">Reason</span>
            <select value={reason} onChange={(e) => setReason(e.target.value as ReportReason)} className={inputClass}>
              {REASONS.map((r) => <option key={r} value={r}>{REASON_LABELS[r]}</option>)}
            </select>
          </label>
          <fieldset className="space-y-1">
            <legend className="font-medium">Where it comes from</legend>
            <label className="flex items-center gap-2">
              <input type="radio" checked={source === "own_initiative"} onChange={() => setSource("own_initiative")} />
              The team came across it
            </label>
            <label className="flex items-center gap-2">
              <input type="radio" checked={source === "authority_order"} onChange={() => setSource("authority_order")} />
              An authority&rsquo;s order
            </label>
          </fieldset>
          {source === "authority_order" && (
            <div className="grid gap-2 sm:grid-cols-2">
              <input value={authority} onChange={(e) => setAuthority(e.target.value)} required placeholder="Authority, e.g. Bundeskriminalamt" maxLength={200} className={inputClass} />
              <input value={reference} onChange={(e) => setReference(e.target.value)} placeholder="Reference (Aktenzeichen)" maxLength={200} className={inputClass} />
            </div>
          )}
          <textarea
            value={details}
            onChange={(e) => setDetails(e.target.value)}
            placeholder="What you found, or what the order says (internal)"
            maxLength={2000}
            rows={3}
            className={inputClass}
          />
          <Button type="submit" disabled={busy}>Open case</Button>
        </form>
      </main>
    </div>
  );
}

export default function Page() {
  return (
    <Suspense fallback={null}>
      <NewCasePage />
    </Suspense>
  );
}
