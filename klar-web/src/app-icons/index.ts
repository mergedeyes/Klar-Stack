import catalog from "./catalog.json";

// Klar's app icons, from the branding folder (tools/build-app-icons.mjs
// writes catalog.json and public/app-icons/<id>/). Which one a device uses
// is a choice made on that device, under Settings -> Language and app icon,
// and kept in a cookie: a home screen icon belongs to a device, not to an
// account, so nothing about it is stored on the server.

export type Localized = { de: string; en: string };
export type AppIcon = (typeof catalog.icons)[number];

export const APP_ICONS: AppIcon[] = catalog.icons;
export const APP_ICON_CATEGORIES = catalog.categories;
export const DEFAULT_APP_ICON = catalog.default;
export const APP_ICON_COOKIE = "klar_app_icon";

/** The icon with this id, or the default for anything else (no cookie,
 * or an icon that has since been removed). */
export function appIcon(id: string | null | undefined): AppIcon {
  return APP_ICONS.find((icon) => icon.id === id) ?? APP_ICONS.find((icon) => icon.id === DEFAULT_APP_ICON)!;
}

export function appIconFiles(id: string) {
  const base = `/app-icons/${id}`;
  return {
    png192: `${base}/192.png`,
    png512: `${base}/512.png`,
    maskable: `${base}/maskable.png`,
    apple: `${base}/apple.png`,
    // One manifest per icon, at its own path rather than behind a query
    // string the CDN might ignore when caching.
    manifest: `${base}/manifest.webmanifest`,
  };
}

export function localized(text: Localized, locale: string): string {
  return locale === "de" ? text.de : text.en;
}
