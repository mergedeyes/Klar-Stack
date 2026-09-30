import { Landmark, Lock } from "lucide-react";
import type { EvidenceSummary } from "@/lib/api";

// Whether the record should go to the authorities (set by the removal's
// classification) and whether that report was recorded.
export function AuthorityReportBadge({ record }: { record: EvidenceSummary }) {
  if (!record.authority_report) return null;
  if (record.authority_reported) {
    return (
      <span className="flex items-center gap-1 rounded bg-muted px-1.5 py-0.5 text-xs text-muted-foreground">
        <Landmark size={11} /> Reported to authorities
      </span>
    );
  }
  return record.authority_report === "required" ? (
    <span className="flex items-center gap-1 rounded bg-destructive/15 px-1.5 py-0.5 text-xs font-semibold text-destructive">
      <Landmark size={11} /> Report to authorities: required
    </span>
  ) : (
    <span className="flex items-center gap-1 rounded bg-amber-500/15 px-1.5 py-0.5 text-xs font-semibold text-amber-600">
      <Landmark size={11} /> Report to authorities: recommended
    </span>
  );
}

// Where an evidence record stands: purged, on hold, awaiting a decision, or
// kept until its retention ends.
export default function EvidenceStatus({ record }: { record: EvidenceSummary }) {
  if (record.purged_at) {
    return <span className="rounded bg-muted px-1.5 py-0.5 text-xs text-muted-foreground">Purged</span>;
  }
  if (record.legal_hold) {
    return (
      <span className="flex items-center gap-1 rounded bg-destructive/15 px-1.5 py-0.5 text-xs font-semibold text-destructive">
        <Lock size={11} /> Legal hold
      </span>
    );
  }
  if (!record.decided_at) {
    return <span className="rounded bg-amber-500/15 px-1.5 py-0.5 text-xs font-semibold text-amber-600">Awaiting decision</span>;
  }
  if (record.decision === "dismissed") {
    return <span className="rounded bg-muted px-1.5 py-0.5 text-xs text-muted-foreground">Dismissed, purging</span>;
  }
  return (
    <span className="rounded bg-muted px-1.5 py-0.5 text-xs text-muted-foreground">
      Kept until {record.retain_until ? new Date(record.retain_until).toLocaleDateString() : "?"}
    </span>
  );
}
