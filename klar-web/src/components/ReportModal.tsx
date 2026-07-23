"use client";

import { useState } from "react";
import { X } from "lucide-react";
import { reportsApi, type ReportReason, type ReportTargetType } from "@/lib/api";
import { Button } from "@/components/ui/button";

interface ReportModalProps {
  targetType: ReportTargetType;
  targetId: string;
  onClose: () => void;
}

const REASONS: { value: ReportReason; label: string }[] = [
  { value: "spam", label: "Spam" },
  { value: "harassment", label: "Harassment or bullying" },
  { value: "hate_speech", label: "Hate speech" },
  { value: "violence", label: "Violence or graphic content" },
  { value: "self_harm", label: "Self-harm or suicide" },
  { value: "sexual_content", label: "Sexual content" },
  { value: "csam", label: "Child sexual abuse material" },
  { value: "impersonation", label: "Impersonation" },
  { value: "other", label: "Something else" },
];

export default function ReportModal({ targetType, targetId, onClose }: ReportModalProps) {
  const [reason, setReason] = useState<ReportReason | null>(null);
  const [details, setDetails] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [done, setDone] = useState(false);

  const handleSubmit = async () => {
    if (!reason || submitting) return;
    setSubmitting(true);
    setError(null);
    try {
      await reportsApi.create(targetType, targetId, reason, details.trim() || undefined);
      setDone(true);
      setTimeout(onClose, 1200);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to submit report");
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4" onClick={onClose}>
      <div
        className="w-full max-w-sm rounded-xl bg-background p-4 shadow-lg"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="mb-3 flex items-center justify-between">
          <h2 className="font-semibold">Report</h2>
          <button onClick={onClose} className="text-muted-foreground hover:text-foreground">
            <X size={18} />
          </button>
        </div>

        {done ? (
          <p className="py-6 text-center text-sm text-muted-foreground">
            Thanks — we&apos;ll review this.
          </p>
        ) : (
          <>
            <p className="mb-3 text-sm text-muted-foreground">Why are you reporting this?</p>
            <div className="mb-3 space-y-1">
              {REASONS.map((r) => (
                <label
                  key={r.value}
                  className={`flex items-center gap-2 rounded-md px-2 py-1.5 text-sm cursor-pointer hover:bg-muted ${
                    reason === r.value ? "bg-muted" : ""
                  }`}
                >
                  <input
                    type="radio"
                    name="report-reason"
                    value={r.value}
                    checked={reason === r.value}
                    onChange={() => setReason(r.value)}
                  />
                  {r.label}
                </label>
              ))}
            </div>

            <textarea
              value={details}
              onChange={(e) => setDetails(e.target.value)}
              placeholder="Additional details (optional)"
              maxLength={1000}
              rows={2}
              className="mb-3 w-full resize-none rounded-md border border-input bg-transparent px-3 py-2 text-sm outline-none placeholder:text-muted-foreground focus:border-ring focus:ring-1 focus:ring-ring"
            />

            {error && <p className="mb-3 text-sm text-destructive">{error}</p>}

            <Button
              className="w-full"
              onClick={handleSubmit}
              disabled={!reason || submitting}
            >
              {submitting ? "Submitting…" : "Submit report"}
            </Button>
          </>
        )}
      </div>
    </div>
  );
}
