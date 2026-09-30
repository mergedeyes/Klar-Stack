import type { AccountMeasure, EvidenceSummary, StrikeSeverity } from "@/lib/api";

// Labels shared by the admin moderation pages (report queue, evidence).

export const REASON_LABELS: Record<string, string> = {
  spam: "Spam",
  harassment: "Harassment or bullying",
  hate_speech: "Hate speech",
  violence: "Violence or graphic content",
  self_harm: "Self-harm or suicide",
  sexual_content: "Sexual content",
  csam: "Child sexual abuse material",
  impersonation: "Impersonation",
  other: "Something else",
  fraud: "Scam or fraud",
  ncii: "Intimate images without consent",
  terrorism: "Terrorism or serious threats",
  illegal_goods: "Illegal goods (drugs, weapons)",
  extremism: "Extremism or glorifying Nazism/fascism",
  copyright: "Copyright or other rights (rights claim)",
};

export const TRIGGER_LABELS: Record<NonNullable<EvidenceSummary["deletion_trigger"]>, string> = {
  moderation_removal: "Removed by moderation",
  user_deletion: "Deleted by the user",
  account_deletion: "Account deleted",
};

// Labels for the severity of a violation type (the catalog itself comes
// from GET /admin/violations).
export const SEVERITY_LABELS: Record<StrikeSeverity, string> = {
  none: "No strike",
  minor: "Minor · 5 pts, 90 days",
  moderate: "Moderate · 20 pts, 180 days",
  serious: "Serious · 40 pts, 1 year",
  severe: "Severe · 100 pts, doesn't expire",
};

export const MEASURE_LABELS: Record<AccountMeasure, string> = {
  warning: "Warning",
  suspend_7d: "Suspend 7 days",
  suspend_30d: "Suspend 30 days",
  ban: "Suspend permanently",
};
