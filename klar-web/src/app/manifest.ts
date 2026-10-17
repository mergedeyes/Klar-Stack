import type { MetadataRoute } from "next";

// What a phone needs to add Klar to the home screen as an app: it opens on
// the feed rather than on whatever page was open when it was added, full
// screen without the browser's bars (hence pull-to-refresh in
// components/PullToRefresh.tsx), under the name "Klar" with its own icon.
// Served outside the passcode gate (proxy.ts): browsers fetch it without
// cookies. The icons are a placeholder until Klar has a logo.
export default function manifest(): MetadataRoute.Manifest {
  return {
    id: "/",
    name: "Klar",
    short_name: "Klar",
    start_url: "/feed",
    scope: "/",
    display: "standalone",
    // The light theme's background (globals.css), for the splash screen
    // while the app starts; the theme-color meta tags in layout.tsx follow
    // light and dark mode once the page is loaded.
    background_color: "#f5f5f5",
    theme_color: "#f5f5f5",
    icons: [
      { src: "/icons/icon-192.png", sizes: "192x192", type: "image/png", purpose: "any" },
      { src: "/icons/icon-512.png", sizes: "512x512", type: "image/png", purpose: "any" },
      // Full-bleed with the letter inside the safe zone, for Android's
      // circle and squircle masks.
      { src: "/icons/icon-maskable-512.png", sizes: "512x512", type: "image/png", purpose: "maskable" },
    ],
  };
}
