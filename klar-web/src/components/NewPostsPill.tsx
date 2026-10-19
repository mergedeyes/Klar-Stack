"use client";

import { ArrowUp } from "lucide-react";
import { useTranslations } from "next-intl";
import type { NewPosts } from "@/hooks/use-new-posts";

/** Floats under the top bar while there are newer posts than the list
 * shows; tapping it is the reader's choice to load them. */
export default function NewPostsPill({ newPosts, onShow }: { newPosts: NewPosts; onShow: () => void }) {
  const t = useTranslations("feed");
  if (newPosts.count === 0) return null;
  return (
    // No height of its own, so showing it doesn't push the posts down.
    <div className="pointer-events-none sticky top-[calc(4rem+env(safe-area-inset-top,0px))] z-10 flex h-0 justify-center">
      <button
        type="button"
        onClick={() => {
          newPosts.dismiss();
          onShow();
          window.scrollTo({ top: 0, behavior: "smooth" });
        }}
        className="pointer-events-auto mt-2 flex items-center gap-1.5 rounded-full bg-primary px-4 py-1.5 text-sm font-medium text-primary-foreground shadow-md"
      >
        <ArrowUp size={14} />
        {newPosts.more ? t("newPostsMore", { count: newPosts.count }) : t("newPosts", { count: newPosts.count })}
      </button>
    </div>
  );
}
