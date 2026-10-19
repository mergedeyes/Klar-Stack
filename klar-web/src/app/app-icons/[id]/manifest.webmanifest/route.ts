import type { MetadataRoute } from "next";
import { APP_ICONS, appIconFiles } from "@/app-icons";

// What a phone needs to add Klar to the home screen as an app: it opens on
// the feed rather than on whatever page was open when it was added, full
// screen without the browser's bars (hence pull-to-refresh in
// components/PullToRefresh.tsx), under the name "Klar". One per app icon
// (app-icons/index.ts); the root layout links the one this device chose.
// Served outside the passcode gate (proxy.ts): browsers fetch it without
// cookies. The same id on all of them, so switching icons updates the app
// instead of adding a second one.

export const dynamic = "force-static";
export const dynamicParams = false;

export function generateStaticParams() {
  return APP_ICONS.map((icon) => ({ id: icon.id }));
}

export async function GET(_request: Request, { params }: { params: Promise<{ id: string }> }) {
  const files = appIconFiles((await params).id);
  const manifest: MetadataRoute.Manifest = {
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
      { src: files.png192, sizes: "192x192", type: "image/png", purpose: "any" },
      { src: files.png512, sizes: "512x512", type: "image/png", purpose: "any" },
      // Full-bleed with the mark inside the safe zone, for Android's circle
      // and squircle masks.
      { src: files.maskable, sizes: "512x512", type: "image/png", purpose: "maskable" },
    ],
  };
  return new Response(JSON.stringify(manifest), {
    headers: { "Content-Type": "application/manifest+json" },
  });
}
