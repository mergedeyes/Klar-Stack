"use client";

import { useEffect, useState } from "react";
import Image from "next/image";
import { feedbackApi, type FeedbackEntry } from "@/lib/api";

// Screenshots attached to one feedback entry, for the admin view. They're
// only served to admins through the API, so each is fetched with the
// session into an object URL (revoked when the entry leaves the page).
// A click opens the full image in a new tab.
export default function FeedbackScreenshots({ shots }: { shots: FeedbackEntry["screenshots"] }) {
  const [urls, setUrls] = useState<Record<string, string>>({});
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    let cancelled = false;
    const created: string[] = [];
    Promise.all(
      shots.map(async (shot) => {
        const url = await feedbackApi.screenshotUrl(shot.id);
        created.push(url);
        if (!cancelled) setUrls((prev) => ({ ...prev, [shot.id]: url }));
      })
    ).catch(() => { if (!cancelled) setFailed(true); });
    return () => {
      cancelled = true;
      created.forEach(URL.revokeObjectURL);
    };
  }, [shots]);

  if (shots.length === 0) return null;
  return (
    <div className="mb-2">
      <div className="flex gap-2">
        {shots.map((shot) =>
          urls[shot.id] ? (
            <a key={shot.id} href={urls[shot.id]} target="_blank" rel="noreferrer" aria-label="Open screenshot">
              <Image
                src={urls[shot.id]}
                alt="Screenshot"
                width={shot.width}
                height={shot.height}
                unoptimized
                className="h-28 w-auto rounded-md border border-border object-contain"
              />
            </a>
          ) : (
            <div key={shot.id} className="h-28 w-16 animate-pulse rounded-md bg-muted" />
          )
        )}
      </div>
      {failed && <p className="mt-1 text-xs text-destructive">Some screenshots couldn&apos;t be loaded.</p>}
    </div>
  );
}
