"use client";

import { Suspense, useEffect, useState } from "react";
import { useRouter, useSearchParams } from "next/navigation";
import { Bug, CheckCircle2, Lightbulb, MessageCircle } from "lucide-react";
import { useAuth } from "@/lib/auth-context";
import { feedbackApi, type FeedbackCategory } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { SmartBackButton } from "@/components/SmartBackButton";

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

  useEffect(() => {
    if (!authLoading && !user) router.push("/login");
  }, [user, authLoading, router]);

  if (authLoading || !user) return null;

  // Read at render time; this page only renders in the browser once the
  // session is known, so window is available here.
  const context = {
    page_path: fromPath ?? "",
    user_agent: navigator.userAgent,
    viewport: `${window.innerWidth}×${window.innerHeight}`,
  };

  const submit = async () => {
    setSending(true);
    setError(null);
    try {
      await feedbackApi.send(category, message, includeContext ? context : null);
      setSent(true);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Couldn't send your feedback");
    } finally {
      setSending(false);
    }
  };

  const active = CATEGORIES.find((c) => c.value === category)!;

  return (
    <div className="min-h-screen bg-background">
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
