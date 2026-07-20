"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { useAuth } from "@/lib/auth-context";
import { posts as postsApi, type Post } from "@/lib/api";
import PostCard from "@/components/PostCard";
import PostModal from "@/components/PostModal";
import { Button } from "@/components/ui/button";
import TopNav from "@/components/TopNav";

function PostSkeleton() {
  return (
    <div className="animate-pulse border-b border-border py-4">
      <div className="mb-3 flex items-center gap-3">
        <div className="h-9 w-9 rounded-full bg-muted" />
        <div className="h-3 w-24 rounded bg-muted" />
      </div>
      <div className="mb-3 h-48 rounded-lg bg-muted" />
      <div className="mb-2 h-3 w-3/4 rounded bg-muted" />
      <div className="h-3 w-1/2 rounded bg-muted" />
    </div>
  );
}

export default function FeedPage() {
  const { user, loading: authLoading } = useAuth();
  const router = useRouter();

  const [feedPosts, setFeedPosts] = useState<Post[]>([]);
  const [loading, setLoading] = useState(true);
  const [loadingMore, setLoadingMore] = useState(false);
  const [hasMore, setHasMore] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const cursorRef = useRef<string | undefined>(undefined);

  const [activePost, setActivePost] = useState<Post | null>(null);

  useEffect(() => {
    if (!authLoading && !user) router.push("/login");
  }, [user, authLoading, router]);

  const refreshFeed = useCallback(() => {
    cursorRef.current = undefined;
    setLoading(true);
    setError(null);
    setHasMore(true);

    postsApi.feed(undefined, 20)
      .then((page) => {
        setFeedPosts(page);
        if (page.length < 20) {
          setHasMore(false);
        } else {
          cursorRef.current = page[page.length - 1].created_at;
        }
      })
      .catch((err) => {
        setError(err instanceof Error ? err.message : "Failed to load feed");
      })
      .finally(() => {
        setLoading(false);
      });
  }, []);

  useEffect(() => {
    if (authLoading || !user) return;
    refreshFeed();
  }, [authLoading, user, refreshFeed]);

  const loadMore = useCallback(async () => {
    if (!cursorRef.current) return;
    try {
      const page = await postsApi.feed(cursorRef.current, 20);
      setFeedPosts((prev) => [...prev, ...page]);
      if (page.length < 20) {
        setHasMore(false);
      } else {
        cursorRef.current = page[page.length - 1].created_at;
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to load more");
    }
  }, []);

  const sentinelRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const sentinel = sentinelRef.current;
    if (!sentinel) return;
    const observer = new IntersectionObserver(
      async (entries) => {
        if (entries[0].isIntersecting && hasMore && !loadingMore) {
          setLoadingMore(true);
          await loadMore();
          setLoadingMore(false);
        }
      },
      { threshold: 0.1 }
    );
    observer.observe(sentinel);
    return () => observer.disconnect();
  }, [hasMore, loadingMore, loadMore]);

  const handleLikeChange = useCallback(
    (postId: string, liked: boolean, count: number) => {
      setFeedPosts((prev) =>
        prev.map((p) => p.id === postId ? { ...p, _liked: liked, _likeCount: count } as Post & { _liked: boolean; _likeCount: number } : p)
      );
    },
    []
  );

  const showSkeletons = authLoading || loading;

  return (
    <div className="min-h-screen bg-background">
      <TopNav active="feed" onPostCreated={refreshFeed} />

      <main className="mx-auto max-w-3xl px-4">
        {showSkeletons && (
          <>
            <PostSkeleton />
            <PostSkeleton />
            <PostSkeleton />
          </>
        )}

        {!showSkeletons && error && (
          <div className="py-8 text-center text-sm text-destructive">{error}</div>
        )}

        {!showSkeletons && !error && feedPosts.length === 0 && (
          <div className="py-16 text-center">
            <p className="text-lg font-semibold">Your feed is empty</p>
            <p className="mt-1 text-sm text-muted-foreground">
              Follow some people to see their posts here.
            </p>
            <Button className="mt-4" variant="outline" onClick={() => router.push("/search")}>
              Find people to follow
            </Button>
          </div>
        )}

        {feedPosts.map((p) => {
          const post = p as Post & { _liked?: boolean; _likeCount?: number };
          return (
            <PostCard
              key={post.id}
              post={post}
              liked={post._liked}
              likeCount={post._likeCount}
              onOpenModal={setActivePost}
              onLikeChange={handleLikeChange}
            />
          );
        })}

        <div ref={sentinelRef} className="py-4">
          {loadingMore && (
            <div className="flex justify-center">
              <div className="h-5 w-5 animate-spin rounded-full border-2 border-muted border-t-foreground" />
            </div>
          )}
          {!hasMore && feedPosts.length > 0 && (
            <p className="text-center text-xs text-muted-foreground">
              You&apos;re all caught up
            </p>
          )}
        </div>
      </main>

      {activePost && (
        <PostModal
          post={activePost}
          onClose={() => setActivePost(null)}
          onLikeChange={handleLikeChange}
          onDeleted={(postId) => {
            setFeedPosts((prev) => prev.filter((p) => p.id !== postId));
            setActivePost(null);
          }}
        />
      )}
    </div>
  );
}
