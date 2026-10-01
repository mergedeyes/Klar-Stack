"use client";

import { useState } from "react";
import { X } from "lucide-react";
import { blocks, reportsApi, type ReportReason, type ReportTargetType } from "@/lib/api";
import { Button } from "@/components/ui/button";

interface ReportModalProps {
  targetType: ReportTargetType;
  targetId: string;
  onClose: () => void;
  // Whose post, comment, message or profile it is: after reporting, the
  // reporter is offered to block them.
  authorUsername?: string;
}

const REASONS: { value: ReportReason; label: string }[] = [
  { value: "spam", label: "Spam" },
  { value: "harassment", label: "Harassment or bullying" },
  { value: "hate_speech", label: "Hate speech or dehumanising content" },
  { value: "extremism", label: "Extremism or glorifying Nazism/fascism" },
  { value: "violence", label: "Violence or graphic content" },
  { value: "self_harm", label: "Self-harm or suicide" },
  { value: "sexual_content", label: "Sexual content" },
  { value: "ncii", label: "Intimate images shared without consent" },
  { value: "csam", label: "Child sexual abuse material" },
  { value: "terrorism", label: "Terrorism or threats of serious violence" },
  { value: "fraud", label: "Scam or fraud" },
  { value: "illegal_goods", label: "Selling drugs, weapons or other illegal goods" },
  { value: "impersonation", label: "Impersonation" },
  { value: "other", label: "Something else" },
];

// What someone reporting this should know right away, beyond "we'll look
// at it": where to get help, and what not to do.
const GUIDANCE: Partial<Record<ReportReason, React.ReactNode>> = {
  self_harm: (
    <>
      If someone may be in danger right now, call <strong>112</strong>. The TelefonSeelsorge is there around the clock,
      free and anonymous — for them and for you: <strong>0800 111 0 111</strong>, <strong>0800 111 0 222</strong> or{" "}
      <strong>116 123</strong>, and by chat at{" "}
      <a href="https://www.telefonseelsorge.de" target="_blank" rel="noopener noreferrer" className="underline">
        telefonseelsorge.de
      </a>
      .
    </>
  ),
  csam: (
    <>
      Please don&rsquo;t download, screenshot or forward it — not even to report it: that can be a crime in itself, and our
      team sees the original. You can also report it to{" "}
      <a href="https://www.jugendschutz.net/verstoss-melden" target="_blank" rel="noopener noreferrer" className="underline">
        jugendschutz.net
      </a>{" "}
      or the police.
    </>
  ),
  ncii: (
    <>
      If the images show you: you don&rsquo;t need to send us a copy. HateAid helps for free (
      <a href="https://hateaid.org" target="_blank" rel="noopener noreferrer" className="underline">
        hateaid.org
      </a>
      ), and you can report it to the police.
    </>
  ),
  terrorism: <>If someone is in immediate danger, call the police on <strong>110</strong>.</>,
  harassment: <>Blocking the person stops them from following, messaging or interacting with you — you can do that after reporting.</>,
};

export default function ReportModal({ targetType, targetId, onClose, authorUsername }: ReportModalProps) {
  const [reason, setReason] = useState<ReportReason | null>(null);
  const [details, setDetails] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [done, setDone] = useState(false);
  const [blocked, setBlocked] = useState(false);

  const handleSubmit = async () => {
    if (!reason || submitting) return;
    setSubmitting(true);
    setError(null);
    try {
      await reportsApi.create(targetType, targetId, reason, details.trim() || undefined);
      setDone(true);
      // With a block offer on screen, the dialog stays until it is closed.
      if (!authorUsername) setTimeout(onClose, 1200);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to submit report");
    } finally {
      setSubmitting(false);
    }
  };

  const block = async () => {
    if (!authorUsername) return;
    try {
      await blocks.block(authorUsername);
      setBlocked(true);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to block");
    }
  };

  const guidance = reason ? GUIDANCE[reason] : null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4" onClick={onClose}>
      <div
        role="dialog"
        aria-label="Report"
        className="max-h-[90vh] w-full max-w-sm overflow-y-auto rounded-xl bg-background p-4 shadow-lg"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="mb-3 flex items-center justify-between">
          <h2 className="font-semibold">Report</h2>
          <button onClick={onClose} aria-label="Close" className="text-muted-foreground hover:text-foreground">
            <X size={18} />
          </button>
        </div>

        {done ? (
          <div className="space-y-3 py-2 text-sm">
            <p className="text-center text-muted-foreground">Thanks — we&apos;ll review this.</p>
            {guidance && <p className="rounded-md bg-amber-500/10 p-2">{guidance}</p>}
            {authorUsername && (
              blocked ? (
                <p className="text-center">You&rsquo;ve blocked @{authorUsername}.</p>
              ) : (
                <div className="rounded-md border border-border p-2">
                  <p className="mb-2">
                    Block @{authorUsername}? You won&rsquo;t see each other&rsquo;s posts, and they can&rsquo;t follow or
                    message you.
                  </p>
                  <Button size="sm" variant="outline" className="w-full" onClick={block}>
                    Block @{authorUsername}
                  </Button>
                </div>
              )
            )}
            {error && <p className="text-sm text-destructive">{error}</p>}
            {authorUsername && (
              <Button className="w-full" onClick={onClose}>Done</Button>
            )}
          </div>
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

            {guidance && <p className="mb-3 rounded-md bg-amber-500/10 p-2 text-sm">{guidance}</p>}

            {targetType === "message" && (
              <p className="mb-3 text-xs text-muted-foreground">
                Reporting a message lets our team read it and the ten messages before it, and keeps a copy even if
                it&rsquo;s deleted.
              </p>
            )}

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
