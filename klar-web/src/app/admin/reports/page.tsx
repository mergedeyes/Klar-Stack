"use client";

import { useEffect, useState } from "react";
import { useRouter } from "next/navigation";
import Link from "next/link";
import { ShieldAlert, Trash2, X } from "lucide-react";
import { useAuth } from "@/lib/auth-context";
import { adminReportsApi, type AdminReport } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { SmartBackButton } from "@/components/SmartBackButton";
import { getMediaUrl } from "@/lib/utils/media";

const REASON_LABELS: Record<string, string> = {
  spam: "Spam",
  harassment: "Harassment or bullying",
  hate_speech: "Hate speech",
  violence: "Violence or graphic content",
  self_harm: "Self-harm or suicide",
  sexual_content: "Sexual content",
  csam: "Child sexual abuse material",
  impersonation: "Impersonation",
  other: "Something else",
};

const CRITICAL_REASONS = new Set(["csam"]);
const HIGH_REASONS = new Set(["violence", "self_harm", "sexual_content"]);

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

  useEffect(() => {
    if (!authLoading && !user) router.push("/login");
  }, [user, authLoading, router]);

  useEffect(() => {
    if (!user) return;
    adminReportsApi.list()
      .then(setReports)
      .catch((err) => setError(err instanceof Error ? err.message : "Failed to load reports"))
      .finally(() => setLoading(false));
  }, [user]);

  const handleDismiss = async (report: AdminReport) => {
    setBusyId(report.id);
    try {
      await adminReportsApi.dismiss(report.id);
      setReports((prev) => prev.filter((r) => r.id !== report.id));
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to dismiss report");
    } finally {
      setBusyId(null);
    }
  };

  const handleRemove = async (report: AdminReport) => {
    if (!window.confirm("Remove this content? This can't be undone.")) return;
    setBusyId(report.id);
    try {
      await adminReportsApi.remove(report.id);
      setReports((prev) => prev.filter((r) => r.id !== report.id));
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to remove content");
    } finally {
      setBusyId(null);
    }
  };

  if (authLoading || !user) return null;

  return (
    <div className="min-h-screen bg-background">
      <header className="sticky top-0 z-10 flex h-14 items-center gap-3 border-b border-border bg-background/80 px-4 backdrop-blur">
        <SmartBackButton aria-label="Back" />
        <span className="font-semibold">Reports</span>
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
                  <span className="text-sm font-medium">{REASON_LABELS[report.reason] ?? report.reason}</span>
                </div>
                <span className="text-xs text-muted-foreground">
                  {new Date(report.created_at).toLocaleString()}
                </span>
              </div>

              <p className="mb-2 text-xs text-muted-foreground">
                Reported by <strong>{report.reporter_username}</strong>
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
                    <img
                      src={getMediaUrl(report.target_thumb_url)}
                      alt=""
                      className="h-14 w-14 shrink-0 rounded object-cover"
                    />
                  )}
                  {report.target_preview && (
                    <p className="line-clamp-3 text-sm">{report.target_preview}</p>
                  )}
                </div>
              )}

              {report.details && (
                <p className="mb-2 rounded-md bg-muted/30 p-2 text-sm italic">&ldquo;{report.details}&rdquo;</p>
              )}

              <div className="flex gap-2">
                <Button
                  size="sm"
                  variant="outline"
                  onClick={() => handleDismiss(report)}
                  disabled={busyId === report.id}
                >
                  <X size={14} className="mr-1" /> Dismiss
                </Button>
                {report.target_type !== "user" && (
                  <Button
                    size="sm"
                    variant="destructive"
                    onClick={() => handleRemove(report)}
                    disabled={busyId === report.id}
                  >
                    <Trash2 size={14} className="mr-1" /> Remove content
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
