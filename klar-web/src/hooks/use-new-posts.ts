"use client";

import { useCallback, useEffect, useEffectEvent, useState } from "react";
import type { Post } from "@/lib/api";
import { useNotifications } from "@/hooks/use-notifications";

// How often an open feed looks for newer posts while it's on screen.
const CHECK_EVERY_MS = 2 * 60 * 1000;

function isNewer(a: Post, b: Post): boolean {
  const at = Date.parse(a.created_at);
  const bt = Date.parse(b.created_at);
  // Same millisecond: post ids are UUIDv7, so they sort by creation too.
  return at > bt || (at === bt && a.id > b.id);
}

export interface NewPosts {
  /** How many posts are newer than the one at the top; 0 for none. */
  count: number;
  /** Every post of the first page is new, so there may be more. */
  more: boolean;
  /** Hides the count once the new posts are shown. */
  dismiss: () => void;
}

/**
 * Counts the posts that came in above the newest one on screen, for the
 * "new posts" pill. Nothing is inserted on its own, so the list never moves
 * under the reader. Checks when the app catches up (back from the
 * background, see use-notifications.ts) and every two minutes while the
 * page is visible. Compares times rather than ids, so a deleted top post
 * doesn't count every post as new.
 */
export function useNewPosts(newest: Post | undefined, fetchFirstPage: () => Promise<Post[]>): NewPosts {
  const { resyncedAt } = useNotifications();
  const [found, setFound] = useState({ count: 0, more: false });

  // Counted against whatever is on top when the answer arrives.
  const record = useEffectEvent((page: Post[]) => {
    if (!newest) return;
    const count = page.filter((p) => isNewer(p, newest)).length;
    setFound({ count, more: count > 0 && count === page.length });
  });

  useEffect(() => {
    if (!resyncedAt) return;
    fetchFirstPage().then(record).catch(() => {
      // Tried again at the next check.
    });
  }, [resyncedAt, fetchFirstPage]);

  useEffect(() => {
    const timer = setInterval(() => {
      if (document.visibilityState !== "visible") return;
      fetchFirstPage().then(record).catch(() => {});
    }, CHECK_EVERY_MS);
    return () => clearInterval(timer);
  }, [fetchFirstPage]);

  const dismiss = useCallback(() => setFound({ count: 0, more: false }), []);
  return { ...found, dismiss };
}
