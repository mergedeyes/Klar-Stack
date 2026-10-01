"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { useAuth } from "@/lib/auth-context";

export default function Footer() {
  const { user } = useAuth();
  const pathname = usePathname();

  return (
    // Sticky: pinned to the screen's bottom edge while a long page scrolls
    // behind it, and resting below the last item at the end, so it never
    // covers content for good. One line of fixed height; on narrow screens
    // the links scroll sideways instead of wrapping into a taller bar; the
    // faded right edge hints at that, and the extra right padding lets the
    // last link scroll clear of the fade.
    <footer className="sticky bottom-0 z-20 h-10 shrink-0 overflow-x-auto border-t border-border bg-background/80 text-xs text-muted-foreground backdrop-blur max-sm:[mask-image:linear-gradient(to_right,black_80%,transparent)]">
      <nav className="mx-auto flex h-full w-max items-center gap-3 whitespace-nowrap px-4 max-sm:pr-16">
        {/* Signed-in only (sending needs an account). Passes the current
            page along, so a bug report says where it happened. */}
        {user && pathname !== "/feedback" && (
          <>
            <Link href={`/feedback?from=${encodeURIComponent(pathname)}`} className="font-medium text-foreground hover:underline">
              Feedback
            </Link>
            <span aria-hidden="true">·</span>
          </>
        )}
        <Link href="/impressum" className="hover:underline">
          Impressum
        </Link>
        <span aria-hidden="true">·</span>
        <Link href="/datenschutz" className="hover:underline">
          Datenschutz
        </Link>
        <span aria-hidden="true">·</span>
        <Link href="/nutzungsbedingungen" className="hover:underline">
          Nutzungsbedingungen
        </Link>
        <span aria-hidden="true">·</span>
        <Link href="/transparenz" className="hover:underline">
          Transparenz
        </Link>
        <span aria-hidden="true">·</span>
        <Link href="/notices" className="hover:underline">
          Rechtswidrige Inhalte melden
        </Link>
        <span aria-hidden="true">·</span>
        <Link href="/rights" className="hover:underline">
          Rechteverletzung melden
        </Link>
      </nav>
    </footer>
  );
}
