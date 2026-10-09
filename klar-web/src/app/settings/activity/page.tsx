"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import Image from "next/image";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { Heart, MessageCircle, Trash2 } from "lucide-react";
import { useAuth } from "@/lib/auth-context";
import {
  activity,
  comments as commentsApi,
  type ActivityComment,
  type LikedPost,
  type PostCursor,
} from "@/lib/api";
import { getMediaUrl } from "@/lib/utils/media";
import { timeAgo } from "@/lib/utils/time";
import { Button } from "@/components/ui/button";
import PostModal from "@/components/PostModal";
import { SmartBackButton } from "@/components/SmartBackButton";

// "Your activity": the posts the user liked and the comments they wrote,
// newest first, to find them again or take them back. Only posts the user
// can still see are listed (handlers/activity.rs); the data download has
// everything.

// A multiple of three so every page of likes fills whole grid rows.
const PAGE_SIZE = 30;

/**
 * One list that loads its next page when the sentinel scrolls into view,
 * like the profile grid. `items` can be changed locally (an unlike, a
 * deleted comment) without disturbing the cursor.
 */
function usePagedList<T>(load: (cursor?: PostCursor) => Promise<T[]>, cursorOf: (item: T) => PostCursor) {
  const [items, setItems] = useState<T[] | null>(null);
  const [hasMore, setHasMore] = useState(false);
  const [loading, setLoading] = useState(false);
  const [failed, setFailed] = useState(false);
  const cursorRef = useRef<PostCursor | undefined>(undefined);
  const sentinelRef = useRef<HTMLDivElement>(null);

  const applyPage = useCallback((page: T[], first: boolean) => {
    const full = page.length === PAGE_SIZE;
    cursorRef.current = full ? cursorOf(page[page.length - 1]) : undefined;
    setHasMore(full);
    setItems((prev) => (first || !prev ? page : [...prev, ...page]));
  }, [cursorOf]);

  const loadPage = useCallback(async (first: boolean) => {
    setLoading(true);
    setFailed(false);
    try {
      applyPage(await load(first ? undefined : cursorRef.current), first);
    } catch {
      setFailed(true);
    } finally {
      setLoading(false);
    }
  }, [load, applyPage]);

  // The first page; `items` stays null (the spinner) until it arrives.
  useEffect(() => {
    let cancelled = false;
    load()
      .then((page) => { if (!cancelled) applyPage(page, true); })
      .catch(() => { if (!cancelled) setFailed(true); });
    return () => { cancelled = true; };
  }, [load, applyPage]);

  useEffect(() => {
    const sentinel = sentinelRef.current;
    if (!sentinel || !hasMore || loading || failed) return;
    const observer = new IntersectionObserver(
      (entries) => { if (entries[0]?.isIntersecting) loadPage(false); },
      { rootMargin: "400px" },
    );
    observer.observe(sentinel);
    return () => observer.disconnect();
  }, [hasMore, loading, failed, loadPage, items]);

  return { items, setItems, hasMore, loading, failed, sentinelRef, retry: () => loadPage(items === null) };
}

function Spinner() {
  return (
    <div className="flex justify-center py-6">
      <div className="h-5 w-5 animate-spin rounded-full border-2 border-muted border-t-foreground" />
    </div>
  );
}

function ListEnd({
  what,
  failed,
  hasMore,
  loading,
  sentinelRef,
  retry,
}: {
  what: string;
  failed: boolean;
  hasMore: boolean;
  loading: boolean;
  sentinelRef: React.RefObject<HTMLDivElement | null>;
  retry: () => void;
}) {
  if (failed) {
    return (
      <div className="flex justify-center py-6">
        <Button variant="outline" size="sm" onClick={retry}>
          Couldn&apos;t load {what} — retry
        </Button>
      </div>
    );
  }
  if (!hasMore) return null;
  return <div ref={sentinelRef}>{loading && <Spinner />}</div>;
}

function Empty({ icon: Icon, text }: { icon: typeof Heart; text: string }) {
  return (
    <div className="py-16 text-center">
      <Icon size={32} className="mx-auto mb-3 text-muted-foreground" />
      <p className="text-sm text-muted-foreground">{text}</p>
    </div>
  );
}

const likeCursor = (post: LikedPost): PostCursor => ({ time: post.liked_at, id: post.id });
const commentCursor = (comment: ActivityComment): PostCursor => ({ time: comment.created_at, id: comment.id });

function LikesTab() {
  const { items, setItems, ...paging } = usePagedList(activity.likes, likeCursor);
  const [open, setOpen] = useState<LikedPost | null>(null);

  if (items === null) return paging.failed ? <ListEnd what="your likes" {...paging} /> : <Spinner />;

  return (
    <>
      {items.length === 0 ? (
        <Empty icon={Heart} text="Posts you like show up here." />
      ) : (
        <div className="grid grid-cols-3 gap-1">
          {items.map((post) => {
            const thumb = post.medium_url ? getMediaUrl(post.medium_url) : null;
            return (
              <button
                key={post.id}
                onClick={() => setOpen(post)}
                aria-label={`Post by ${post.username}`}
                className="group relative aspect-square w-full overflow-hidden bg-muted focus:outline-none"
              >
                {thumb ? (
                  <Image src={thumb} alt={post.caption ?? "Post"} fill className="object-cover group-hover:opacity-80" unoptimized />
                ) : (
                  <span className="flex h-full w-full items-center justify-center p-2">
                    <span className="line-clamp-4 text-center text-xs text-muted-foreground">{post.caption ?? ""}</span>
                  </span>
                )}
              </button>
            );
          })}
        </div>
      )}
      <ListEnd what="more likes" {...paging} />
      {open && (
        <PostModal
          post={open}
          onClose={() => setOpen(null)}
          // An unlike takes the post off this list straight away.
          onLikeChange={(postId, liked) => {
            if (!liked) setItems((prev) => prev?.filter((p) => p.id !== postId) ?? null);
          }}
        />
      )}
    </>
  );
}

function CommentRow({ comment, onDeleted }: { comment: ActivityComment; onDeleted: () => void }) {
  const [deleting, setDeleting] = useState(false);
  const [error, setError] = useState(false);
  const thumb = comment.post_thumb_url ? getMediaUrl(comment.post_thumb_url) : null;

  const handleDelete = async () => {
    if (!window.confirm("Delete this comment?")) return;
    setDeleting(true);
    setError(false);
    try {
      await commentsApi.delete(comment.post_id, comment.id);
      onDeleted();
    } catch {
      setError(true);
      setDeleting(false);
    }
  };

  return (
    <div className="flex items-start gap-3 px-4 py-3">
      <Link href={`/posts/${comment.post_id}`} className="flex min-w-0 flex-1 items-start gap-3">
        <span className="relative h-11 w-11 shrink-0 overflow-hidden rounded-md bg-muted">
          {thumb && <Image src={thumb} alt="" fill className="object-cover" unoptimized />}
        </span>
        <span className="min-w-0 flex-1">
          <span className="block text-xs text-muted-foreground">
            {comment.parent_comment_id ? "Reply on" : "On"} {comment.post_username}&apos;s post · {timeAgo(comment.created_at)}
          </span>
          <span className="line-clamp-3 block whitespace-pre-wrap break-words text-sm">{comment.body}</span>
          {comment.moderation_status === "hidden" && (
            <span className="block text-xs text-destructive">Hidden — only you can see this comment</span>
          )}
          {error && <span className="block text-xs text-destructive">Couldn&apos;t delete — try again</span>}
        </span>
      </Link>
      <Button
        variant="ghost"
        size="icon"
        onClick={handleDelete}
        disabled={deleting}
        aria-label="Delete comment"
        className="shrink-0 text-muted-foreground hover:text-destructive"
      >
        <Trash2 size={16} />
      </Button>
    </div>
  );
}

function CommentsTab() {
  const { items, setItems, ...paging } = usePagedList(activity.comments, commentCursor);

  if (items === null) return paging.failed ? <ListEnd what="your comments" {...paging} /> : <Spinner />;

  return (
    <>
      {items.length === 0 ? (
        <Empty icon={MessageCircle} text="Comments you write show up here." />
      ) : (
        <div className="divide-y divide-border overflow-hidden rounded-xl border border-border">
          {items.map((comment) => (
            <CommentRow
              key={comment.id}
              comment={comment}
              onDeleted={() => setItems((prev) => prev?.filter((c) => c.id !== comment.id) ?? null)}
            />
          ))}
        </div>
      )}
      <ListEnd what="more comments" {...paging} />
    </>
  );
}

const TABS = [
  { id: "likes", label: "Likes", icon: Heart },
  { id: "comments", label: "Comments", icon: MessageCircle },
] as const;

export default function ActivityPage() {
  const { user, loading: authLoading } = useAuth();
  const router = useRouter();
  const [tab, setTab] = useState<(typeof TABS)[number]["id"]>("likes");

  useEffect(() => {
    if (!authLoading && !user) router.push("/login");
  }, [user, authLoading, router]);

  if (authLoading || !user) return null;

  return (
    <div className="flex-1 bg-background">
      <header className="sticky top-0 z-10 border-b border-border bg-background/80 backdrop-blur">
        <div className="mx-auto flex h-14 max-w-lg items-center gap-3 px-4">
          <SmartBackButton aria-label="Back" />
          <span className="font-semibold">Your activity</span>
        </div>
      </header>

      <main className="mx-auto max-w-lg px-4 py-3">
        <div role="tablist" className="mb-3 grid grid-cols-2 border-b border-border">
          {TABS.map(({ id, label, icon: Icon }) => (
            <button
              key={id}
              role="tab"
              aria-selected={tab === id}
              onClick={() => setTab(id)}
              className={`flex items-center justify-center gap-1.5 border-b-2 py-2.5 text-sm font-medium transition-colors ${
                tab === id ? "border-foreground text-foreground" : "border-transparent text-muted-foreground hover:text-foreground"
              }`}
            >
              <Icon size={16} />
              {label}
            </button>
          ))}
        </div>
        <div role="tabpanel">{tab === "likes" ? <LikesTab /> : <CommentsTab />}</div>
      </main>
    </div>
  );
}
