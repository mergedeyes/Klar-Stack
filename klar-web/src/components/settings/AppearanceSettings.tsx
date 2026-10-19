"use client";

import { useEffect, useState } from "react";
import Image from "next/image";
import { useRouter } from "next/navigation";
import { Check } from "lucide-react";
import { useLocale, useTranslations } from "next-intl";
import { useAuth } from "@/lib/auth-context";
import { users } from "@/lib/api";
import { SmartBackButton } from "@/components/SmartBackButton";
import { LOCALES, LOCALE_NAMES, isLocale } from "@/i18n/config";
import { APP_ICONS, APP_ICON_CATEGORIES, DEFAULT_APP_ICON, appIconFiles, localized, type AppIcon } from "@/app-icons";
import { rememberAppIcon } from "@/app-icons/cookie";

/** A category's icons, in runs that share a group (e.g. 80s, 90s, Y2K). */
function byGroup(icons: AppIcon[]) {
  const runs: { group: AppIcon["group"]; icons: AppIcon[] }[] = [];
  for (const icon of icons) {
    const last = runs[runs.length - 1];
    if (last && last.group?.de === icon.group?.de) last.icons.push(icon);
    else runs.push({ group: icon.group, icons: [icon] });
  }
  return runs;
}

export default function AppearanceSettings({ currentIcon }: { currentIcon: string }) {
  const { user, loading: authLoading, refreshUser } = useAuth();
  const router = useRouter();
  const t = useTranslations("appearance");
  const tc = useTranslations("common");
  const locale = useLocale();
  const [savingLocale, setSavingLocale] = useState(false);
  const [icon, setIcon] = useState(currentIcon);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!authLoading && !user) router.push("/login");
  }, [user, authLoading, router]);

  if (authLoading || !user) return null;

  // Saved to the account; LocaleSync then renders the page in it.
  const handleLocale = async (next: string) => {
    if (!isLocale(next)) return;
    setSavingLocale(true);
    setError(null);
    try {
      await users.setLocale(next);
      await refreshUser();
    } catch (err) {
      setError(err instanceof Error ? err.message : tc("couldntSave"));
    } finally {
      setSavingLocale(false);
    }
  };

  // Kept on this device; the page is rendered again so the tab icon and
  // the manifest's icons follow at once.
  const chooseIcon = (id: string) => {
    setIcon(id);
    rememberAppIcon(id);
    router.refresh();
  };

  return (
    <div className="flex-1 bg-background">
      <header className="sticky top-0 z-10 border-b border-border bg-background/80 backdrop-blur">
        <div className="mx-auto flex h-14 max-w-lg items-center gap-3 px-4">
          <SmartBackButton aria-label={tc("back")} />
          <h1 className="font-semibold">{t("title")}</h1>
        </div>
      </header>

      <main className="mx-auto max-w-lg space-y-8 px-4 py-4">
        {error && <div className="rounded-md bg-destructive/10 px-3 py-2 text-sm text-destructive">{error}</div>}

        <section>
          <label className="flex items-center justify-between gap-3">
            <span className="text-sm font-medium">{t("language")}</span>
            <select
              value={locale}
              onChange={(e) => handleLocale(e.target.value)}
              disabled={savingLocale}
              aria-label={t("language")}
              className="rounded-md border border-input bg-background px-2 py-1 text-sm disabled:opacity-60"
            >
              {LOCALES.map((l) => (
                <option key={l} value={l} lang={l}>
                  {LOCALE_NAMES[l]}
                </option>
              ))}
            </select>
          </label>
        </section>

        <section aria-labelledby="app-icon-heading">
          <h2 id="app-icon-heading" className="text-sm font-medium">
            {t("appIcon")}
          </h2>
          <p className="mt-1 text-xs text-muted-foreground">{t("thisDevice")}</p>

          <div className="mt-4 space-y-6">
            {APP_ICON_CATEGORIES.map((category) => {
              const icons = APP_ICONS.filter((i) => i.category === category.id);
              return (
                <div key={category.id} role="group" aria-label={localized(category.name, locale)}>
                  <h3 className="mb-2 text-xs font-medium uppercase tracking-wide text-muted-foreground">
                    {localized(category.name, locale)}
                  </h3>
                  {byGroup(icons).map((run, i) => (
                    <div key={run.group?.de ?? i} className="mb-3">
                      {run.group && run.group.de !== category.name.de && (
                        <p className="mb-1.5 text-xs text-muted-foreground">{localized(run.group, locale)}</p>
                      )}
                      <div className="grid grid-cols-4 gap-3 sm:grid-cols-6">
                        {run.icons.map((option) => {
                          const name = localized(option.name, locale);
                          const chosen = option.id === icon;
                          return (
                            <button
                              key={option.id}
                              type="button"
                              onClick={() => chooseIcon(option.id)}
                              aria-pressed={chosen}
                              aria-label={run.group ? `${localized(run.group, locale)} ${name}` : name}
                              className="flex flex-col items-center gap-1 text-center"
                            >
                              <span
                                className={`relative block rounded-[22%] p-0.5 ring-2 transition ${chosen ? "ring-foreground" : "ring-transparent"}`}
                              >
                                <Image
                                  src={appIconFiles(option.id).png192}
                                  alt=""
                                  width={56}
                                  height={56}
                                  unoptimized
                                  className="h-14 w-14"
                                />
                                {chosen && (
                                  <span className="absolute -right-1 -top-1 flex h-5 w-5 items-center justify-center rounded-full bg-foreground text-background">
                                    <Check size={12} />
                                  </span>
                                )}
                              </span>
                              <span className="text-[11px] leading-tight text-muted-foreground">
                                {name}
                                {option.id === DEFAULT_APP_ICON && ` · ${t("default")}`}
                              </span>
                            </button>
                          );
                        })}
                      </div>
                    </div>
                  ))}
                </div>
              );
            })}
          </div>

          <p className="mt-2 text-xs leading-relaxed text-muted-foreground">{t("homeScreen")}</p>
        </section>
      </main>
    </div>
  );
}
