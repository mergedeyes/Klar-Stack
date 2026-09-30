"use client";

import { Suspense, useEffect, useRef, useState } from "react";
import Image from "next/image";
import { useRouter, useSearchParams } from "next/navigation";
import { Bug, CheckCircle2, ImagePlus, Lightbulb, MessageCircle, X } from "lucide-react";
import { useAuth } from "@/lib/auth-context";
import {
  feedbackApi,
  FEEDBACK_MAX_SCREENSHOT_BYTES,
  FEEDBACK_MAX_SCREENSHOTS,
  type FeedbackCategory,
} from "@/lib/api";
import { Button } from "@/components/ui/button";
import { SmartBackButton } from "@/components/SmartBackButton";

const SCREENSHOT_TYPES = ["image/jpeg", "image/png", "image/webp"];

const CATEGORIES: { value: FeedbackCategory; label: string; icon: typeof Bug; hint: string }[] = [
  { value: "bug", label: "Something's broken", icon: Bug, hint: "What did you do, what happened, and what did you expect?" },
  { value: "idea", label: "Idea", icon: Lightbulb, hint: "What would make Klar better for you?" },
  { value: "other", label: "Other", icon: MessageCircle, hint: "Anything else on your mind?" },
];

function FeedbackForm() {
  const { user, loading: authLoading } = useAuth();
  const router = useRouter();
  const searchParams = useSearchParams();
  // The page the footer link was clicked on. Only a path: the query string
  // could carry tokens, and the backend strips it again anyway.
  const fromPath = (searchParams.get("from") ?? "").split(/[?#]/)[0] || null;

  const [category, setCategory] = useState<FeedbackCategory>("bug");
  const [message, setMessage] = useState("");
  const [includeContext, setIncludeContext] = useState(true);
  const [sending, setSending] = useState(false);
  const [sent, setSent] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // Picked screenshots with a preview URL each, revoked when removed.
  const [screenshots, setScreenshots] = useState<{ file: File; url: string }[]>([]);
  const fileInput = useRef<HTMLInputElement>(null);
  // The latest list, for revoking what's left when the page unmounts.
  const previews = useRef(screenshots);
  useEffect(() => { previews.current = screenshots; }, [screenshots]);

  useEffect(() => {
    if (!authLoading && !user) router.push("/login");
  }, [user, authLoading, router]);

  useEffect(() => () => previews.current.forEach((s) => URL.revokeObjectURL(s.url)), []);

  if (authLoading || !user) return null;

  // Read at render time; this page only renders in the browser once the
  // session is known, so window is available here.
  const context = {
    page_path: fromPath ?? "",
    user_agent: navigator.userAgent,
    viewport: `${window.innerWidth}×${window.innerHeight}`,
  };

  const addScreenshots = (files: FileList | null) => {
    if (!files) return;
    setError(null);
    const room = FEEDBACK_MAX_SCREENSHOTS - screenshots.length;
    const picked = Array.from(files);
    const valid = picked.filter((f) => SCREENSHOT_TYPES.includes(f.type) && f.size <= FEEDBACK_MAX_SCREENSHOT_BYTES);
    if (valid.length < picked.length) setError("Screenshots must be JPEG, PNG or WebP images under 10 MB.");
    else if (valid.length > room) setError(`You can attach up to ${FEEDBACK_MAX_SCREENSHOTS} screenshots.`);
    setScreenshots((prev) => [...prev, ...valid.slice(0, room).map((file) => ({ file, url: URL.createObjectURL(file) }))]);
    // Picking the same file again after removing it should work too.
    if (fileInput.current) fileInput.current.value = "";
  };

  const removeScreenshot = (url: string) => {
    URL.revokeObjectURL(url);
    setScreenshots((prev) => prev.filter((s) => s.url !== url));
  };

  const submit = async () => {
    setSending(true);
    setError(null);
    try {
      await feedbackApi.send(category, message, includeContext ? context : null, screenshots.map((s) => s.file));
      screenshots.forEach((s) => URL.revokeObjectURL(s.url));
      setScreenshots([]);
      setSent(true);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Couldn't send your feedback");
    } finally {
      setSending(false);
    }
  };

  const active = CATEGORIES.find((c) => c.value === category)!;

  return (
    <div className="flex-1 bg-background">
      <header className="sticky top-0 z-10 flex h-14 items-center gap-3 border-b border-border bg-background/80 px-4 backdrop-blur">
        <SmartBackButton aria-label="Back" />
        <span className="font-semibold">Feedback</span>
      </header>

      <main className="mx-auto max-w-xl px-4 py-6">
        {sent ? (
          <div className="rounded-xl border border-border p-6 text-center">
            <CheckCircle2 size={32} className="mx-auto mb-3 text-muted-foreground" />
            <p className="font-semibold">Thank you!</p>
            <p className="mt-1 text-sm text-muted-foreground">Your feedback went straight to the Klar team.</p>
            <div className="mt-4 flex justify-center gap-2">
              <Button variant="outline" size="sm" onClick={() => { setSent(false); setMessage(""); }}>
                Send more
              </Button>
              <Button size="sm" onClick={() => router.push(fromPath ?? "/feed")}>Back to Klar</Button>
            </div>
          </div>
        ) : (
          <form onSubmit={(e) => { e.preventDefault(); submit(); }} className="space-y-4">
            <p className="text-sm text-muted-foreground">
              Klar is still being tested. Bugs, rough edges, ideas — everything helps. Only the Klar team reads this.
            </p>

            <div className="grid grid-cols-3 gap-2" role="radiogroup" aria-label="Kind of feedback">
              {CATEGORIES.map(({ value, label, icon: Icon }) => (
                <button
                  key={value}
                  type="button"
                  role="radio"
                  aria-checked={category === value}
                  onClick={() => setCategory(value)}
                  className={`flex flex-col items-center gap-1 rounded-lg border p-3 text-xs font-medium ${
                    category === value ? "border-foreground bg-muted" : "border-border hover:bg-muted/50"
                  }`}
                >
                  <Icon size={18} />
                  {label}
                </button>
              ))}
            </div>

            <textarea
              value={message}
              onChange={(e) => setMessage(e.target.value)}
              placeholder={active.hint}
              maxLength={4000}
              rows={6}
              aria-label="Your feedback"
              className="w-full rounded-md border border-input bg-transparent px-3 py-2 text-sm outline-none placeholder:text-muted-foreground focus:border-ring focus:ring-1 focus:ring-ring"
            />

            <div>
              {screenshots.length > 0 && (
                <div className="mb-2 flex gap-2">
                  {screenshots.map((s) => (
                    <div key={s.url} className="relative">
                      <Image
                        src={s.url}
                        alt="Screenshot preview"
                        width={80}
                        height={112}
                        unoptimized
                        className="h-28 w-20 rounded-md border border-border object-cover"
                      />
                      <button
                        type="button"
                        onClick={() => removeScreenshot(s.url)}
                        aria-label="Remove screenshot"
                        className="absolute -right-2 -top-2 rounded-full border border-border bg-background p-0.5 shadow"
                      >
                        <X size={14} />
                      </button>
                    </div>
                  ))}
                </div>
              )}
              <input
                ref={fileInput}
                type="file"
                accept={SCREENSHOT_TYPES.join(",")}
                multiple
                hidden
                onChange={(e) => addScreenshots(e.target.files)}
                aria-label="Screenshot files"
              />
              {screenshots.length < FEEDBACK_MAX_SCREENSHOTS && (
                <Button type="button" variant="outline" size="sm" onClick={() => fileInput.current?.click()}>
                  <ImagePlus size={16} /> Add screenshot
                </Button>
              )}
              <p className="mt-1 text-xs text-muted-foreground">
                Up to {FEEDBACK_MAX_SCREENSHOTS}. Screenshots can show other people&apos;s posts or messages, so only
                attach what helps. They&apos;re deleted 30 days after we&apos;ve dealt with your feedback, after 90 days at
                the latest.
              </p>
            </div>

            <label className="flex items-start gap-2 text-sm">
              <input
                type="checkbox"
                checked={includeContext}
                onChange={(e) => setIncludeContext(e.target.checked)}
                className="mt-0.5"
              />
              <span>
                Include technical details
                <span className="block text-xs text-muted-foreground">
                  {fromPath ? `Page: ${fromPath} · ` : ""}Screen: {context.viewport} · Browser: {context.user_agent}
                </span>
              </span>
            </label>

            {error && (
              <div className="rounded-md bg-destructive/10 px-3 py-2 text-sm text-destructive">{error}</div>
            )}

            <Button type="submit" disabled={sending || message.trim().length < 5}>
              {sending ? "Sending…" : "Send feedback"}
            </Button>
          </form>
        )}
      </main>
    </div>
  );
}

export default function FeedbackPage() {
  return (
    <Suspense fallback={null}>
      <FeedbackForm />
    </Suspense>
  );
}
