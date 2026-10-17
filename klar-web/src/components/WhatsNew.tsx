"use client";

import { useEffect, useRef, useState } from "react";
import Link from "next/link";
import { usePathname } from "next/navigation";
import { Sparkles, X } from "lucide-react";
import { useLocale, useTranslations } from "next-intl";
import { useAuth } from "@/lib/auth-context";
import { users } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { releaseDate, releaseToShow, type Release } from "@/whats-new/releases";
import { isLocale } from "@/i18n/config";

/** One release's items, in the page's language; shared with /whats-new. */
export function ReleaseItems({ release, onNavigate }: { release: Release; onNavigate?: () => void }) {
  const locale = useLocale();
  const t = useTranslations("whatsNew");
  const text = release[isLocale(locale) ? locale : "en"];
  return (
    <ul className="space-y-4">
      {text.items.map((item) => (
        <li key={item.title}>
          <p className="text-sm font-semibold">{item.title}</p>
          <p className="mt-0.5 text-sm leading-relaxed text-muted-foreground">{item.text}</p>
          {item.href && (
            <Link href={item.href} onClick={onNavigate} className="mt-1 inline-block text-sm font-medium underline underline-offset-4">
              {t("tryIt")}
            </Link>
          )}
        </li>
      ))}
    </ul>
  );
}

// The newest release, once per account, over the feed: where sign-in
// lands, so it never covers a legal page or a link from an email. Waits
// for the welcome guide, and for GET /users/me, which says what the
// account has seen (users.whats_new_seen, or its sign-up day).
export default function WhatsNew() {
  const { user } = useAuth();
  const pathname = usePathname();
  // The release closed in this tab, so it doesn't come back before
  // GET /users/me says it was seen.
  const [closedId, setClosedId] = useState<string | null>(null);

  const release = user && user.guide_seen !== false ? releaseToShow(user.whats_new_seen) : undefined;
  if (!release || release.id === closedId || pathname !== "/feed") return null;

  const close = () => {
    setClosedId(release.id);
    // If this fails, it comes back next time; nothing worse.
    users.markWhatsNewSeen(release.id).catch(() => {});
  };
  return <WhatsNewDialog release={release} onClose={close} />;
}

function WhatsNewDialog({ release, onClose }: { release: Release; onClose: () => void }) {
  const t = useTranslations("whatsNew");
  const locale = useLocale();
  const okRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    okRef.current?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <div className="fixed inset-0 z-40 flex items-center justify-center bg-black/50 p-4">
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="whats-new-title"
        className="relative flex max-h-[90dvh] w-full max-w-md flex-col overflow-y-auto rounded-xl bg-background p-5 shadow-xl"
      >
        <button
          onClick={onClose}
          className="absolute right-3 top-3 rounded-md p-1 text-muted-foreground hover:bg-muted hover:text-foreground"
          aria-label={t("close")}
        >
          <X size={18} />
        </button>
        <span className="mb-3 flex h-10 w-10 items-center justify-center rounded-full border border-border">
          <Sparkles size={20} />
        </span>
        <h2 id="whats-new-title" className="text-lg font-semibold">
          {t("title")}
        </h2>
        <p className="mb-4 text-xs text-muted-foreground">{releaseDate(release, locale)}</p>
        <div className="mb-5">
          <ReleaseItems release={release} onNavigate={onClose} />
        </div>
        <div className="flex items-center gap-2">
          <Button variant="ghost" asChild>
            <Link href="/whats-new" onClick={onClose}>
              {t("allUpdates")}
            </Link>
          </Button>
          <Button ref={okRef} className="flex-1" onClick={onClose}>
            {t("gotIt")}
          </Button>
        </div>
      </div>
    </div>
  );
}
