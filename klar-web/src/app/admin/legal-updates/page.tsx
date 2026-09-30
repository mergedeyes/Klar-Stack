"use client";

import { useCallback, useEffect, useState } from "react";
import { useRouter } from "next/navigation";
import { FileText } from "lucide-react";
import { useAuth } from "@/lib/auth-context";
import { adminLegalUpdatesApi, type AdminLegalUpdate, type LegalDocument } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { SmartBackButton } from "@/components/SmartBackButton";

const DOCUMENT_LABELS: Record<LegalDocument, string> = {
  terms: "Nutzungsbedingungen",
  privacy: "Datenschutzerklärung",
};

// Tell every existing account about changed Terms or privacy policy: a
// notice in the app on their next visit (Terms changes must be accepted
// there) and an email to verified addresses.
export default function AdminLegalUpdatesPage() {
  const { user, loading: authLoading } = useAuth();
  const router = useRouter();

  const [updates, setUpdates] = useState<AdminLegalUpdate[] | null>(null);
  const [documents, setDocuments] = useState<LegalDocument[]>([]);
  const [summary, setSummary] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [done, setDone] = useState<string | null>(null);

  useEffect(() => {
    if (!authLoading && !user) router.push("/login");
  }, [user, authLoading, router]);

  const load = useCallback(() => {
    adminLegalUpdatesApi.list()
      .then(setUpdates)
      .catch((err) => setError(err instanceof Error ? err.message : "Failed to load"));
  }, []);

  useEffect(() => {
    if (user) load();
  }, [user, load]);

  const toggle = (d: LegalDocument) =>
    setDocuments((prev) => (prev.includes(d) ? prev.filter((x) => x !== d) : [...prev, d]));

  const publish = async (e: React.FormEvent) => {
    e.preventDefault();
    const acceptance = documents.includes("terms")
      ? " Everyone who already has an account has to accept the new Terms to keep using Klar."
      : "";
    if (!window.confirm(`Publish this notice and email all verified accounts?${acceptance}`)) return;
    setBusy(true);
    setError(null);
    setDone(null);
    try {
      await adminLegalUpdatesApi.publish(documents, summary.trim());
      setDone("Published. Emails are going out in the background.");
      setDocuments([]);
      setSummary("");
      load();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to publish");
    } finally {
      setBusy(false);
    }
  };

  if (authLoading || !user) return null;

  return (
    <div className="flex-1 bg-background">
      <header className="sticky top-0 z-10 flex h-14 items-center gap-3 border-b border-border bg-background/80 px-4 backdrop-blur">
        <SmartBackButton aria-label="Back" />
        <span className="font-semibold">Legal updates</span>
      </header>

      <main className="mx-auto max-w-2xl px-4 py-4">
        {error && <div className="mb-4 rounded-md bg-destructive/10 px-3 py-2 text-sm text-destructive">{error}</div>}
        {done && <div className="mb-4 rounded-md bg-muted px-3 py-2 text-sm" role="status">{done}</div>}

        <form onSubmit={publish} className="mb-6 space-y-3 rounded-xl border border-border p-3">
          <h2 className="flex items-center gap-1.5 text-sm font-semibold">
            <FileText size={14} /> Announce a change
          </h2>
          <p className="text-xs text-muted-foreground">
            Publish after the new version of the page is live. Every account that already exists sees the notice on
            its next visit; a Terms change has to be accepted there, and the acceptance is recorded. Verified
            addresses also get an email (unverified ones may be a stranger&apos;s).
          </p>
          <fieldset className="flex flex-wrap gap-4 text-sm">
            <legend className="sr-only">Changed documents</legend>
            {(Object.keys(DOCUMENT_LABELS) as LegalDocument[]).map((d) => (
              <label key={d} className="flex items-center gap-2">
                <input type="checkbox" checked={documents.includes(d)} onChange={() => toggle(d)} disabled={busy} />
                {DOCUMENT_LABELS[d]}
              </label>
            ))}
          </fieldset>
          <textarea
            value={summary}
            onChange={(e) => setSummary(e.target.value)}
            placeholder="Was ist neu? In einfacher Sprache, auf Deutsch (mindestens 20 Zeichen)"
            aria-label="Summary"
            maxLength={2000}
            rows={4}
            disabled={busy}
            className="w-full rounded-md border border-input bg-transparent px-2 py-1.5 text-sm outline-none placeholder:text-muted-foreground focus:border-ring focus:ring-1 focus:ring-ring"
          />
          <Button size="sm" type="submit" disabled={busy || documents.length === 0 || summary.trim().length < 20}>
            Publish and email
          </Button>
        </form>

        <h2 className="mb-2 text-sm font-semibold">Published</h2>
        {updates?.length === 0 && <p className="text-sm text-muted-foreground">Nothing published yet.</p>}
        <div className="space-y-3">
          {updates?.map((u) => (
            <div key={u.id} className="rounded-xl border border-border p-3 text-sm">
              <p className="font-medium">
                {u.documents.map((d) => DOCUMENT_LABELS[d]).join(" + ")}
                {u.requires_acceptance && <span className="font-normal text-muted-foreground"> · must be accepted</span>}
              </p>
              <p className="text-xs text-muted-foreground">
                {new Date(u.published_at).toLocaleString()} · {u.acknowledged} of {u.audience} accounts{" "}
                {u.requires_acceptance ? "accepted" : "saw it"} · {u.emails_sent} emails
                {u.emails_finished_at ? " (done)" : " (sending…)"}
              </p>
              <p className="mt-2 whitespace-pre-wrap">{u.summary}</p>
            </div>
          ))}
        </div>
      </main>
    </div>
  );
}
