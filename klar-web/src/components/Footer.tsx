"use client";

import Link from "next/link";

export default function Footer() {
  return (
    <footer className="border-t border-border py-4 px-4 text-center text-xs text-muted-foreground">
      <nav className="flex items-center justify-center gap-4 flex-wrap">
        <Link href="/impressum" className="hover:underline">
          Impressum
        </Link>
        <span aria-hidden="true">·</span>
        <Link href="/datenschutz" className="hover:underline">
          Datenschutz
        </Link>
      </nav>
    </footer>
  );
}
