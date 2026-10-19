"use client";

import { useEffect } from "react";

const COVER = "viewport-fit=cover";

// In the home screen app, lets the page draw under the status bar, the
// camera cutout and the gesture bar instead of Android leaving black bars
// there; globals.css (top-safe, the body's padding) and the footer keep
// the content clear of them. Only there: set in a browser tab, a static
// viewport-fit=cover breaks Chrome's install sheet. Not on iPhones, whose
// home screen app already starts below the status bar.
export default function EdgeToEdge() {
  useEffect(() => {
    if ("standalone" in navigator) return;
    const installed = window.matchMedia("(display-mode: standalone), (display-mode: fullscreen)");

    const apply = () => {
      const meta = document.querySelector<HTMLMetaElement>('meta[name="viewport"]');
      if (!meta) return;
      const parts = meta.content
        .split(",")
        .map((part) => part.trim())
        .filter((part) => part && part !== COVER);
      const content = (installed.matches ? [...parts, COVER] : parts).join(", ");
      if (meta.content !== content) meta.content = content;
    };

    apply();
    // React writes the tag back when the root layout renders again (e.g.
    // router.refresh() after a language change), so it's watched.
    const observer = new MutationObserver(apply);
    observer.observe(document.head, { subtree: true, childList: true, attributes: true, attributeFilter: ["content"] });
    installed.addEventListener("change", apply);
    return () => {
      observer.disconnect();
      installed.removeEventListener("change", apply);
    };
  }, []);

  return null;
}
