"use client";

import { useEffect, useRef, useState } from "react";
import { Check, Share2 } from "lucide-react";

/**
 * Shares a post's permalink (/posts/:id). Uses the system share sheet where
 * there is one (phones), otherwise copies the link and says so. The link
 * itself grants nothing: a private account's post still only opens for
 * people allowed to see it.
 */
export default function ShareButton({
  postId,
  username,
  size = 18,
  className = "",
}: {
  postId: string;
  username?: string;
  size?: number;
  /** For placing the button (e.g. "ml-auto"); applied to its wrapper. */
  className?: string;
}) {
  const [copied, setCopied] = useState(false);
  // Shown when the clipboard is unavailable (e.g. an insecure context), so
  // the link can still be copied by hand.
  const [manualUrl, setManualUrl] = useState<string | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => () => { if (timer.current) clearTimeout(timer.current); }, []);

  const share = async (e: React.MouseEvent) => {
    // Cards and modals react to clicks too; sharing shouldn't open or close them.
    e.stopPropagation();
    const url = `${window.location.origin}/posts/${postId}`;

    // The share sheet only on touch devices: desktop browsers that support
    // it open a heavy system dialog where copying is what people expect.
    const touch = window.matchMedia("(pointer: coarse)").matches;
    if (touch && typeof navigator.share === "function") {
      try {
        await navigator.share({ title: username ? `Post by @${username} on Klar` : "Post on Klar", url });
        return;
      } catch (err) {
        // Dismissing the sheet is not an error; anything else falls back to copying.
        if (err instanceof DOMException && err.name === "AbortError") return;
      }
    }

    try {
      await navigator.clipboard.writeText(url);
      setCopied(true);
      if (timer.current) clearTimeout(timer.current);
      timer.current = setTimeout(() => setCopied(false), 2000);
    } catch {
      setManualUrl(url);
    }
  };

  return (
    <span className={`relative inline-flex ${className}`}>
      <button
        type="button"
        onClick={share}
        className="flex items-center gap-1.5 text-sm text-muted-foreground transition-colors hover:text-foreground"
        aria-label="Share post"
        title="Share post"
      >
        {copied ? <Check size={size} /> : <Share2 size={size} />}
        {copied && <span>Link copied</span>}
      </button>
      {/* Announces the copy to screen readers. */}
      <span className="sr-only" aria-live="polite">{copied ? "Link copied" : ""}</span>

      {manualUrl && (
        <span
          className="absolute bottom-full right-0 z-30 mb-2 flex w-64 items-center gap-2 rounded-md border border-border bg-background p-2 shadow-lg"
          onClick={(e) => e.stopPropagation()}
        >
          <input
            readOnly
            value={manualUrl}
            autoFocus
            onFocus={(e) => e.target.select()}
            aria-label="Link to this post"
            className="min-w-0 flex-1 bg-transparent text-xs outline-none"
          />
          <button type="button" onClick={() => setManualUrl(null)} className="text-xs text-muted-foreground hover:text-foreground">
            Close
          </button>
        </span>
      )}
    </span>
  );
}
