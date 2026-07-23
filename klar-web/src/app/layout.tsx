import type { Metadata, Viewport } from "next";
import { Geist } from "next/font/google";
import "./globals.css";
import { AuthProvider } from "@/lib/auth-context";
import { NotificationsProvider } from "@/hooks/use-notifications";
import Footer from "@/components/Footer";

const geist = Geist({ subsets: ["latin"] });

export const metadata: Metadata = {
  title: "Klar",
  description: "Privacy-first photo sharing",
};

// Without this, mobile browsers default to a ~980px desktop-width viewport
// and shrink the whole page to fit it — that's the "too zoomed in / wrong
// size" symptom on mobile. This makes the layout render at the device's
// actual width instead.
export const viewport: Viewport = {
  width: "device-width",
  initialScale: 1,
};

export default function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <html lang="en" suppressHydrationWarning>
      <body className={geist.className}>
        <AuthProvider>
          <NotificationsProvider>{children}</NotificationsProvider>
        </AuthProvider>
        <Footer />
      </body>
    </html>
  );
}
