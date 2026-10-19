"use client";

import { useEffect, useEffectEvent, useState, useSyncExternalStore } from "react";
import { RefreshCw } from "lucide-react";

// How far (after damping) the page has to be pulled to refresh, and how far
// the indicator follows the finger.
const THRESHOLD = 70;
const MAX_PULL = 100;

const STANDALONE = "(display-mode: standalone)";

function subscribe(onChange: () => void) {
  const query = window.matchMedia(STANDALONE);
  query.addEventListener("change", onChange);
  return () => query.removeEventListener("change", onChange);
}

// Opened from the home screen as an app. Safari on iOS says so through
// navigator.standalone rather than the media query.
function isStandalone() {
  return window.matchMedia(STANDALONE).matches || (navigator as Navigator & { standalone?: boolean }).standalone === true;
}

/**
 * Pull down at the top of the page to reload it. Only in the home screen
 * app: a browser tab has its own pull-to-refresh, but the app has no
 * address bar and no reload button, so without this there would be no way
 * to look for new data by hand. Ignored while a modal is open, so
 * scrolling one doesn't reload the page behind it.
 */
export default function PullToRefresh({ onRefresh }: { onRefresh: () => Promise<unknown> }) {
  const standalone = useSyncExternalStore(subscribe, isStandalone, () => false);
  const [pull, setPull] = useState(0);
  const [refreshing, setRefreshing] = useState(false);
  const refresh = useEffectEvent(() => onRefresh());

  useEffect(() => {
    if (!standalone) return;
    let startY: number | null = null;
    let distance = 0;
    let busy = false;

    const onStart = (e: TouchEvent) => {
      const inModal = (e.target as Element | null)?.closest?.('[role="dialog"], [aria-modal="true"]');
      if (busy || inModal || document.body.style.overflow === "hidden" || window.scrollY > 0 || e.touches.length !== 1) {
        startY = null;
        return;
      }
      startY = e.touches[0].clientY;
    };

    const onMove = (e: TouchEvent) => {
      if (startY === null) return;
      const dy = e.touches[0].clientY - startY;
      if (dy <= 0 || window.scrollY > 0) {
        distance = 0;
        setPull(0);
        return;
      }
      // Otherwise iOS bounces the page instead of letting it be pulled.
      e.preventDefault();
      distance = Math.min(dy * 0.5, MAX_PULL);
      setPull(distance);
    };

    const onEnd = async () => {
      if (startY === null) return;
      startY = null;
      const pulled = distance;
      distance = 0;
      if (pulled < THRESHOLD) {
        setPull(0);
        return;
      }
      busy = true;
      setRefreshing(true);
      setPull(THRESHOLD);
      try {
        await refresh();
      } catch {
        // The page shows its own error for a failed load.
      } finally {
        busy = false;
        setRefreshing(false);
        setPull(0);
      }
    };

    document.addEventListener("touchstart", onStart, { passive: true });
    document.addEventListener("touchmove", onMove, { passive: false });
    document.addEventListener("touchend", onEnd);
    document.addEventListener("touchcancel", onEnd);
    return () => {
      document.removeEventListener("touchstart", onStart);
      document.removeEventListener("touchmove", onMove);
      document.removeEventListener("touchend", onEnd);
      document.removeEventListener("touchcancel", onEnd);
    };
  }, [standalone]);

  if (!standalone || (pull === 0 && !refreshing)) return null;
  return (
    <div
      aria-hidden="true"
      data-testid="pull-to-refresh"
      className="pointer-events-none fixed inset-x-0 top-[calc(3.5rem+env(safe-area-inset-top,0px))] z-20 flex justify-center"
      style={{ transform: `translateY(${pull - 40}px)` }}
    >
      <div className="flex h-9 w-9 items-center justify-center rounded-full border border-border bg-background shadow-md">
        <RefreshCw
          size={16}
          className={refreshing ? "animate-spin" : undefined}
          style={refreshing ? undefined : { transform: `rotate(${pull * 3}deg)`, opacity: Math.min(pull / THRESHOLD, 1) }}
        />
      </div>
    </div>
  );
}
