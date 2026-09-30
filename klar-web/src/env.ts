// env.ts
export const ENV = {
  API_URL: process.env.NEXT_PUBLIC_API_URL,
  STORAGE_URL: process.env.NEXT_PUBLIC_STORAGE_URL,
  // The site's own address, for absolute links in link previews.
  SITE_URL: process.env.NEXT_PUBLIC_SITE_URL || "https://www.klarsocial.eu",
  // If a variable isn't in this list, it's not being used by the app.
};