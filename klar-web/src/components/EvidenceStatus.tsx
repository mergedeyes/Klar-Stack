import { Lock } from "lucide-react";
import type { EvidenceSummary } from "@/lib/api";

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
