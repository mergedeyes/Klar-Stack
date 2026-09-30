"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { useAuth } from "@/lib/auth-context";

export default function Footer() {
  const { user } = useAuth();
  const pathname = usePathname();

  return (
    <footer className="border-t border-border py-4 px-4 text-center text-xs text-muted-foreground">
      <nav className="flex items-center justify-center gap-4 flex-wrap">
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
      </nav>
    </footer>
  );
}
