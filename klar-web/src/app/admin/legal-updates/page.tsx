"use client";

import { useEffect, useState } from "react";
import { useRouter } from "next/navigation";
import { FileText } from "lucide-react";
import { useAuth } from "@/lib/auth-context";
import { adminLegalUpdatesApi, type AdminLegalUpdate, type LegalDocument } from "@/lib/api";
import { SmartBackButton } from "@/components/SmartBackButton";

const DOCUMENT_LABELS: Record<LegalDocument, string> = {
  terms: "Nutzungsbedingungen",
  privacy: "Datenschutzerklärung",
};

// Notices about changed Terms or privacy policy, with their progress. They
// aren't written here: each is a file in klar-web/legal-updates, added in
// the pull request that changes the page (CI refuses the change without
// one), and the frontend deploy publishes it once the new page is live.
export default function AdminLegalUpdatesPage() {
  const { user, loading: authLoading } = useAuth();
  const router = useRouter();

  const [updates, setUpdates] = useState<AdminLegalUpdate[] | null>(null);
  const [error, setError] = useState<string | null>(null);

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
        <div className="space-y-3">
          {updates?.map((u) => (
            <div key={u.id} className="rounded-xl border border-border p-3 text-sm">
              <p className="font-medium">
                {u.documents.map((d) => DOCUMENT_LABELS[d]).join(" + ")}
                {u.requires_acceptance && <span className="font-normal text-muted-foreground"> · must be accepted</span>}
              </p>
              <p className="text-xs text-muted-foreground">
                {new Date(u.published_at).toLocaleString()} · {u.source_key ?? "published by hand"} ·{" "}
                {u.acknowledged} of {u.audience} accounts {u.requires_acceptance ? "accepted" : "saw it"} ·{" "}
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
