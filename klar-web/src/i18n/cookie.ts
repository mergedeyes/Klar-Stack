import { LOCALE_COOKIE, type AppLocale } from "./config";

// Browser-side only. A year, renewed on every visit while signed in.
export function rememberLocale(locale: AppLocale) {
  const secure = location.protocol === "https:" ? "; secure" : "";
  document.cookie = `${LOCALE_COOKIE}=${locale}; path=/; max-age=31536000; samesite=lax${secure}`;
}

// Returns whether there was one, i.e. whether the page may now be in
// another language than the browser's.
export function forgetLocale(): boolean {
  const had = document.cookie.split("; ").some((c) => c.startsWith(`${LOCALE_COOKIE}=`));
  if (had) document.cookie = `${LOCALE_COOKIE}=; path=/; max-age=0`;
  return had;
}
