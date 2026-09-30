import type { EvidenceSummary } from "@/lib/api";

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
  copyright: "Copyright or other rights (rights claim)",
};

export const TRIGGER_LABELS: Record<NonNullable<EvidenceSummary["deletion_trigger"]>, string> = {
  moderation_removal: "Removed by moderation",
  user_deletion: "Deleted by the user",
  account_deletion: "Account deleted",
};
