"use client";

import { Suspense, useEffect, useRef, useState } from "react";
import Image from "next/image";
import { useRouter, useSearchParams } from "next/navigation";
import { useTranslations } from "next-intl";
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

// Labels and hints are in the messages (feedback.categories, feedback.hints).
const CATEGORIES: { value: FeedbackCategory; icon: typeof Bug }[] = [
  { value: "bug", icon: Bug },
  { value: "idea", icon: Lightbulb },
  { value: "other", icon: MessageCircle },
];

function FeedbackForm() {
  const { user, loading: authLoading } = useAuth();
  const router = useRouter();
  const searchParams = useSearchParams();
  const t = useTranslations("feedback");
  const tc = useTranslations("common");
  const tf = useTranslations("footer");
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
    if (valid.length < picked.length) setError(t("badScreenshots"));
    else if (valid.length > room) setError(t("tooMany", { max: FEEDBACK_MAX_SCREENSHOTS }));
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
      setError(err instanceof Error ? err.message : t("failed"));
    } finally {
      setSending(false);
    }
  };

  return (
    <div className="flex-1 bg-background">
      <header className="sticky top-safe z-10 flex h-14 items-center gap-3 border-b border-border bg-background/80 px-4 backdrop-blur">
        <SmartBackButton />
        <span className="font-semibold">{tf("feedback")}</span>
      </header>

      <main className="mx-auto max-w-xl px-4 py-6">
        {sent ? (
          <div className="rounded-xl border border-border p-6 text-center">
            <CheckCircle2 size={32} className="mx-auto mb-3 text-muted-foreground" />
            <p className="font-semibold">{t("thanks")}</p>
            <p className="mt-1 text-sm text-muted-foreground">{t("wentToTeam")}</p>
            <div className="mt-4 flex justify-center gap-2">
              <Button variant="outline" size="sm" onClick={() => { setSent(false); setMessage(""); }}>
                {t("sendMore")}
              </Button>
              <Button size="sm" onClick={() => router.push(fromPath ?? "/feed")}>{t("backToKlar")}</Button>
            </div>
          </div>
        ) : (
          <form onSubmit={(e) => { e.preventDefault(); submit(); }} className="space-y-4">
            <p className="text-sm text-muted-foreground">{t("intro")}</p>

            <div className="grid grid-cols-3 gap-2" role="radiogroup" aria-label={t("kind")}>
              {CATEGORIES.map(({ value, icon: Icon }) => (
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
                  {t(`categories.${value}`)}
                </button>
              ))}
            </div>

            <textarea
              value={message}
              onChange={(e) => setMessage(e.target.value)}
              placeholder={t(`hints.${category}`)}
              maxLength={4000}
              rows={6}
              aria-label={t("yourFeedback")}
              className="w-full rounded-md border border-input bg-transparent px-3 py-2 text-sm outline-none placeholder:text-muted-foreground focus:border-ring focus:ring-1 focus:ring-ring"
            />

            <div>
              {screenshots.length > 0 && (
                <div className="mb-2 flex gap-2">
                  {screenshots.map((s) => (
                    <div key={s.url} className="relative">
                      <Image
                        src={s.url}
                        alt={t("screenshotPreview")}
                        width={80}
                        height={112}
                        unoptimized
                        className="h-28 w-20 rounded-md border border-border object-cover"
                      />
                      <button
                        type="button"
                        onClick={() => removeScreenshot(s.url)}
                        aria-label={t("removeScreenshot")}
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
                aria-label={t("screenshotFiles")}
              />
              {screenshots.length < FEEDBACK_MAX_SCREENSHOTS && (
                <Button type="button" variant="outline" size="sm" onClick={() => fileInput.current?.click()}>
                  <ImagePlus size={16} /> {t("addScreenshot")}
                </Button>
              )}
              <p className="mt-1 text-xs text-muted-foreground">
                {t("screenshotNote", { max: FEEDBACK_MAX_SCREENSHOTS })}
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
                {t("includeDetails")}
                <span className="block text-xs text-muted-foreground">
                  {fromPath
                    ? t("detailsWithPage", { page: fromPath, screen: context.viewport, browser: context.user_agent })
                    : t("details", { screen: context.viewport, browser: context.user_agent })}
                </span>
              </span>
            </label>

            {error && (
              <div className="rounded-md bg-destructive/10 px-3 py-2 text-sm text-destructive">{error}</div>
            )}

            <Button type="submit" disabled={sending || message.trim().length < 5}>
              {sending ? tc("sending") : t("send")}
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
