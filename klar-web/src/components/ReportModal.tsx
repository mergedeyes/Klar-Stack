"use client";

import { useState } from "react";
import { useTranslations } from "next-intl";
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

// In the order they're offered; the labels are in the messages
// (report.reasons).
const REASONS: ReportReason[] = [
  "spam",
  "harassment",
  "hate_speech",
  "extremism",
  "violence",
  "self_harm",
  "sexual_content",
  "ncii",
  "csam",
  "terrorism",
  "fraud",
  "illegal_goods",
  "impersonation",
  "other",
];

// What someone reporting this should know right away, beyond "we'll look
// at it": where to get help, and what not to do. The text is in the
// messages (report.guidance); this is where its link goes.
const GUIDANCE_LINKS = {
  self_harm: "https://www.telefonseelsorge.de",
  csam: "https://www.jugendschutz.net/verstoss-melden",
  ncii: "https://hateaid.org",
  terrorism: null,
  harassment: null,
} as const;

type GuidedReason = keyof typeof GUIDANCE_LINKS;

function hasGuidance(reason: ReportReason): reason is GuidedReason {
  return reason in GUIDANCE_LINKS;
}

function Guidance({ reason }: { reason: GuidedReason }) {
  const t = useTranslations("report.guidance");
  const href = GUIDANCE_LINKS[reason];
  return t.rich(reason, {
    b: (chunks) => <strong>{chunks}</strong>,
    link: (chunks) => (
      <a href={href ?? undefined} target="_blank" rel="noopener noreferrer" className="underline">
        {chunks}
      </a>
    ),
  });
}

export default function ReportModal({ targetType, targetId, onClose, authorUsername }: ReportModalProps) {
  const t = useTranslations("report");
  const tc = useTranslations("common");
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
      setError(err instanceof Error ? err.message : t("failed"));
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
      setError(err instanceof Error ? err.message : t("blockFailed"));
    }
  };

  const guidance = reason && hasGuidance(reason) ? <Guidance reason={reason} /> : null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4" onClick={onClose}>
      <div
        role="dialog"
        aria-label={t("title")}
        className="max-h-[90vh] w-full max-w-sm overflow-y-auto rounded-xl bg-background p-4 shadow-lg"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="mb-3 flex items-center justify-between">
          <h2 className="font-semibold">{t("title")}</h2>
          <button onClick={onClose} aria-label={tc("close")} className="text-muted-foreground hover:text-foreground">
            <X size={18} />
          </button>
        </div>

        {done ? (
          <div className="space-y-3 py-2 text-sm">
            <p className="text-center text-muted-foreground">{t("thanks")}</p>
            {guidance && <p className="rounded-md bg-amber-500/10 p-2">{guidance}</p>}
            {authorUsername && (
              blocked ? (
                <p className="text-center">{t("blocked", { username: authorUsername })}</p>
              ) : (
                <div className="rounded-md border border-border p-2">
                  <p className="mb-2">{t("blockOffer", { username: authorUsername })}</p>
                  <Button size="sm" variant="outline" className="w-full" onClick={block}>
                    {t("blockButton", { username: authorUsername })}
                  </Button>
                </div>
              )
            )}
            {error && <p className="text-sm text-destructive">{error}</p>}
            {authorUsername && (
              <Button className="w-full" onClick={onClose}>{t("done")}</Button>
            )}
          </div>
        ) : (
          <>
            <p className="mb-3 text-sm text-muted-foreground">{t("why")}</p>
            <div className="mb-3 space-y-1">
              {REASONS.map((value) => (
                <label
                  key={value}
                  className={`flex items-center gap-2 rounded-md px-2 py-1.5 text-sm cursor-pointer hover:bg-muted ${
                    reason === value ? "bg-muted" : ""
                  }`}
                >
                  <input
                    type="radio"
                    name="report-reason"
                    value={value}
                    checked={reason === value}
                    onChange={() => setReason(value)}
                  />
                  {t(`reasons.${value}`)}
                </label>
              ))}
            </div>

            {guidance && <p className="mb-3 rounded-md bg-amber-500/10 p-2 text-sm">{guidance}</p>}

            {targetType === "message" && (
              <p className="mb-3 text-xs text-muted-foreground">{t("messageNote")}</p>
            )}

            <textarea
              value={details}
              onChange={(e) => setDetails(e.target.value)}
              placeholder={t("details")}
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
              {submitting ? t("submitting") : t("submit")}
            </Button>
          </>
        )}
      </div>
    </div>
  );
}
