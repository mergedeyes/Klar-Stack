"use client";

import { useLocale, useTranslations } from "next-intl";
import { SmartBackButton } from "@/components/SmartBackButton";
import { ReleaseItems } from "@/components/WhatsNew";
import { RELEASES, releaseDate } from "@/whats-new/releases";

// Every "What's new" release, newest first: for anyone who closed the
// dialog too fast, or joined later and is curious.
export default function WhatsNewPage() {
  const t = useTranslations("whatsNew");
  const tc = useTranslations("common");
  const locale = useLocale();

  return (
    <div className="flex-1 bg-background">
      <header className="sticky top-safe z-10 border-b border-border bg-background/80 backdrop-blur">
        <div className="mx-auto flex h-14 max-w-lg items-center gap-3 px-4">
          <SmartBackButton aria-label={tc("back")} />
          <h1 className="font-semibold">{t("title")}</h1>
        </div>
      </header>
      <main className="mx-auto max-w-lg space-y-8 px-4 py-6">
        {RELEASES.length === 0 && <p className="text-sm text-muted-foreground">{t("empty")}</p>}
        {RELEASES.map((release) => (
          <section key={release.id} aria-label={releaseDate(release, locale)}>
            <h2 className="mb-3 text-xs font-medium text-muted-foreground">{releaseDate(release, locale)}</h2>
            <ReleaseItems release={release} />
          </section>
        ))}
      </main>
    </div>
  );
}
