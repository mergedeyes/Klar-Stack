// Klar
// Copyright (C) 2026  Jan Motulla (mergedeyes)
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published
// by the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

import type { Metadata, Viewport } from "next";
import { Geist } from "next/font/google";
import { NextIntlClientProvider } from "next-intl";
import { getLocale, getTranslations } from "next-intl/server";
import { cookies } from "next/headers";
import { APP_ICON_COOKIE, appIcon, appIconFiles } from "@/app-icons";
import "./globals.css";
import { AuthProvider } from "@/lib/auth-context";
import { NotificationsProvider } from "@/hooks/use-notifications";
import Footer from "@/components/Footer";
import LegalUpdateNotice from "@/components/LegalUpdateNotice";
import VerifyEmailBanner from "@/components/VerifyEmailBanner";
import LocaleSync from "@/components/LocaleSync";
import EdgeToEdge from "@/components/EdgeToEdge";
import WhatsNew from "@/components/WhatsNew";
import { GuideProvider } from "@/components/WelcomeGuide";

const geist = Geist({ subsets: ["latin"] });

export async function generateMetadata(): Promise<Metadata> {
  const t = await getTranslations("meta");
  // The app icon this device chose (app-icons/index.ts), for the tab, the
  // home screen and the manifest.
  const icon = appIconFiles(appIcon((await cookies()).get(APP_ICON_COOKIE)?.value).id);
  return {
    title: "Klar",
    description: t("description"),
    manifest: icon.manifest,
    icons: {
      icon: [{ url: icon.png192, sizes: "192x192", type: "image/png" }],
      apple: [{ url: icon.apple, sizes: "180x180", type: "image/png" }],
    },
    // iOS: open from the home screen without Safari's bars, as the
    // manifest asks of the other browsers.
    appleWebApp: { capable: true, title: "Klar", statusBarStyle: "default" },
  };
}

// Without this, mobile browsers default to a ~980px desktop-width viewport
// and shrink the whole page to fit it — that's the "too zoomed in / wrong
// size" symptom on mobile. This makes the layout render at the device's
// actual width instead.
export const viewport: Viewport = {
  width: "device-width",
  initialScale: 1,
  // The page background in light and dark mode (globals.css), for the
  // browser's bars and the home screen app's status bar.
  themeColor: [
    { media: "(prefers-color-scheme: light)", color: "#f5f5f5" },
    { media: "(prefers-color-scheme: dark)", color: "#0a0a0a" },
  ],
};

export default async function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  // The browser's language, or the signed-in account's (i18n/request.ts).
  const locale = await getLocale();
  return (
    <html lang={locale} suppressHydrationWarning>
      {/* A full-height column: the page fills whatever the banner and the
          footer leave, so a short page ends exactly at the screen's bottom
          edge and a long one scrolls with the footer pinned below it. */}
      <body className={`${geist.className} flex min-h-dvh flex-col`}>
        <EdgeToEdge />
        {/* In the home screen app, the status bar's strip in the page's
            colour, so content scrolling up doesn't show behind it (sticky
            headers stop below it, top-safe). No height in a browser tab. */}
        <div aria-hidden="true" className="pointer-events-none fixed inset-x-0 top-0 z-30 h-[env(safe-area-inset-top,0px)] bg-background" />
        <NextIntlClientProvider>
          <AuthProvider>
            <LocaleSync />
            {/* Around the footer too, which has the link that opens the guide. */}
            <GuideProvider>
              <VerifyEmailBanner />
              <div className="flex min-h-0 flex-1 flex-col">
                <NotificationsProvider>{children}</NotificationsProvider>
              </div>
              {/* Inside AuthProvider: it shows the Feedback link to signed-in users. */}
              <Footer />
            </GuideProvider>
            {/* Changed Terms / privacy policy, once per notice. */}
            <LegalUpdateNotice />
            {/* Under the legal notice (z-50), which has to be answered first. */}
            <WhatsNew />
          </AuthProvider>
        </NextIntlClientProvider>
      </body>
    </html>
  );
}
