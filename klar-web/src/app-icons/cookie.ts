import { APP_ICON_COOKIE, DEFAULT_APP_ICON } from "./index";

// Browser-side only. Set only when someone picks an icon other than the
// default, and removed when they go back to it. A year, renewed on every
// change.
export function rememberAppIcon(id: string) {
  if (id === DEFAULT_APP_ICON) {
    document.cookie = `${APP_ICON_COOKIE}=; path=/; max-age=0`;
    return;
  }
  const secure = location.protocol === "https:" ? "; secure" : "";
  document.cookie = `${APP_ICON_COOKIE}=${id}; path=/; max-age=31536000; samesite=lax${secure}`;
}
