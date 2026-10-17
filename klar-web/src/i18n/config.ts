// The languages Klar is translated into; each has a file in messages/. The
// backend accepts the same list (validation.rs, LOCALES).
export const LOCALES = ["en", "de"] as const;
export type AppLocale = (typeof LOCALES)[number];

// For browsers that ask for none of the above.
export const DEFAULT_LOCALE: AppLocale = "en";

// The signed-in account's language, so the server renders pages in it
// rather than in the browser's. Only set once someone is signed in (the
// language is then a setting of their account); everyone else gets the
// browser's language, read from each request, with nothing stored.
export const LOCALE_COOKIE = "klar_locale";

export function isLocale(value: unknown): value is AppLocale {
  return LOCALES.includes(value as AppLocale);
}

// The browser's most preferred language that Klar has, from an
// Accept-Language header such as "de-AT,de;q=0.9,en;q=0.8".
export function negotiateLocale(header: string | null): AppLocale {
  if (!header) return DEFAULT_LOCALE;
  const ranked = header
    .split(",")
    .map((part, index) => {
      const [tag, ...params] = part.split(";").map((s) => s.trim());
      const q = params.find((p) => p.startsWith("q="));
      return { lang: tag.toLowerCase().split("-")[0], weight: q ? Number(q.slice(2)) : 1, index };
    })
    .filter((entry) => entry.weight > 0)
    .sort((a, b) => b.weight - a.weight || a.index - b.index);
  return ranked.map((entry) => entry.lang).find(isLocale) ?? DEFAULT_LOCALE;
}

// Each language under its own name, so it can be found whatever the page
// is shown in.
export const LOCALE_NAMES: Record<AppLocale, string> = {
  en: "English",
  de: "Deutsch",
};
