"use client";

import { useCallback, useEffect, useRef } from "react";
import { useRouter } from "next/navigation";
import { X } from "lucide-react";
import type { Post } from "@/lib/api";
import PostView from "@/components/PostView";

interface PostModalProps {
  post: Post;
  onClose: () => void;
  onLikeChange?: (postId: string, liked: boolean, count: number) => void;
  onDeleted?: (postId: string) => void;
}

/**
 * A post opened on top of a feed or profile. The post itself is PostView;
 * this adds the overlay, the close button, and the address handling below.
 * Opened directly (a shared link, a reload), /posts/:id shows the full
 * post page instead.
 */
export default function PostModal({ post, onClose, onLikeChange, onDeleted }: PostModalProps) {
  const router = useRouter();

  // The open post gets its own address: opening the modal from a feed or a
  // profile pushes /posts/:id, so the address bar shows a link that can be
  // copied, reloaded or shared, and the back button closes the modal. On
  // the permalink page itself the address is already right and nothing is
  // pushed. pushedRef survives React's dev double-mount, so the second run
  // sees the address already set and doesn't push twice.
  const pushedRef = useRef(false);
  // Where to go once the pushed entry has been popped (a profile link
  // clicked inside the modal), so the history reads feed -> profile.
  const afterBackRef = useRef<string | null>(null);
  const onCloseRef = useRef(onClose);
  useEffect(() => { onCloseRef.current = onClose; }, [onClose]);

  useEffect(() => {
    const path = `/posts/${post.id}`;
    if (window.location.pathname !== path) {
      window.history.pushState(null, "", path);
      pushedRef.current = true;
    }
    const onPop = () => {
      if (!pushedRef.current) return;
      pushedRef.current = false;
      onCloseRef.current();
      const next = afterBackRef.current;
      if (next) {
        afterBackRef.current = null;
        router.push(next);
      }
    };
    window.addEventListener("popstate", onPop);
    return () => window.removeEventListener("popstate", onPop);
  }, [post.id, router]);

  // Every way of closing goes through here: step back out of the pushed
  // entry (its popstate closes the modal), or close directly if nothing
  // was pushed.
  const dismiss = useCallback(() => {
    if (pushedRef.current) window.history.back();
    else onCloseRef.current();
  }, []);

  // Links inside the modal (author, commenters, @mentions): leave the
  // pushed entry first, then navigate, so back from the profile returns to
  // the feed rather than to a /posts/:id address without its modal.
  const leaveViaLink = (e: React.MouseEvent) => {
    const anchor = (e.target as HTMLElement).closest("a");
    const href = anchor?.getAttribute("href");
    if (!pushedRef.current || !href?.startsWith("/") || e.metaKey || e.ctrlKey || e.shiftKey) return;
    e.preventDefault();
    e.stopPropagation();
    afterBackRef.current = href;
    window.history.back();
  };
  useEffect(() => {
    const handler = (e: KeyboardEvent) => { if (e.key === "Escape") dismiss(); };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [dismiss]);

  // The page behind doesn't scroll while the modal is open.
  useEffect(() => {
    document.body.style.overflow = "hidden";
    return () => { document.body.style.overflow = ""; };
  }, []);

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4"
      onClick={(e) => { if (e.target === e.currentTarget) dismiss(); }}
      onClickCapture={leaveViaLink}
    >
      <div className="relative flex max-h-[90vh] w-full max-w-4xl flex-col overflow-hidden rounded-xl bg-background shadow-2xl md:flex-row">
        <button onClick={dismiss} className="absolute right-3 top-3 z-10 rounded-full bg-background/80 p-1.5 text-muted-foreground backdrop-blur hover:text-foreground" aria-label="Close">
          <X size={18} />
        </button>
        <PostView
          post={post}
          layout="modal"
          onBack={dismiss}
          afterDelete={() => { onDeleted?.(post.id); dismiss(); }}
          onLikeChange={onLikeChange}
        />
      </div>
    </div>
  );
}
