"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { useAuth } from "@/lib/auth-context";
import { posts as postsApi, type Post, type DiscoveryCursor } from "@/lib/api";
import PostCard from "@/components/PostCard";
import PostModal from "@/components/PostModal";
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

export default function DiscoveryPage() {
  const { user, loading: authLoading } = useAuth();
  const router = useRouter();

  const [posts, setPosts] = useState<Post[]>([]);
  const [loading, setLoading] = useState(true);
  const [loadingMore, setLoadingMore] = useState(false);
  const [hasMore, setHasMore] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const cursorRef = useRef<DiscoveryCursor | undefined>(undefined);

  const [activePost, setActivePost] = useState<Post | null>(null);

  useEffect(() => {
    if (!authLoading && !user) router.push("/login");
  }, [user, authLoading, router]);

  const refreshDiscovery = useCallback(() => {
    cursorRef.current = undefined;
    setLoading(true);
    setError(null);
    setHasMore(true);

    postsApi.discoveryFeed(undefined, 15)
      .then((res) => {
        setPosts(res.data);
        cursorRef.current = res.next_cursor ?? undefined;
        setHasMore(res.next_cursor !== null);
      })
      .catch((err) => {
        setError(err instanceof Error ? err.message : "Failed to load discovery feed");
      })
      .finally(() => {
        setLoading(false);
      });
  }, []);

  useEffect(() => {
    if (authLoading || !user) return;
    refreshDiscovery();
  }, [authLoading, user, refreshDiscovery]);

  const loadMore = useCallback(async () => {
    if (!cursorRef.current) return;
    try {
      const res = await postsApi.discoveryFeed(cursorRef.current, 15);
      setPosts((prev) => [...prev, ...res.data]);
      cursorRef.current = res.next_cursor ?? undefined;
      setHasMore(res.next_cursor !== null);
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
      { rootMargin: "300px", threshold: 0.1 }
    );
    observer.observe(sentinel);
    return () => observer.disconnect();
  }, [hasMore, loadingMore, loadMore]);

  const handleLikeChange = useCallback(
    (postId: string, liked: boolean, count: number) => {
      setPosts((prev) =>
        prev.map((p) => p.id === postId ? { ...p, _liked: liked, _likeCount: count } as Post & { _liked: boolean; _likeCount: number } : p)
      );
    },
    []
  );

  const showSkeletons = authLoading || loading;

  return (
    <div className="min-h-screen bg-background">
      <TopNav active="discovery" onPostCreated={refreshDiscovery} />

      <main className="mx-auto max-w-3xl px-4">
        <div className="py-6">
          <h1 className="text-2xl font-bold tracking-tight">Discovery</h1>
          <p className="mt-1 text-sm text-muted-foreground">
            Posts from across Klar, beyond who you follow.
          </p>
        </div>

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

        {!showSkeletons && !error && posts.length === 0 && (
          <div className="py-16 text-center">
            <p className="text-lg font-semibold">Nothing to discover yet</p>
            <p className="mt-1 text-sm text-muted-foreground">
              Check back once more people are posting on Klar.
            </p>
          </div>
        )}

        {posts.map((p) => {
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
          {!hasMore && posts.length > 0 && (
            <p className="text-center text-xs text-muted-foreground">
              You&apos;ve reached the end 🏁
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
            setPosts((prev) => prev.filter((p) => p.id !== postId));
            setActivePost(null);
          }}
        />
      )}
    </div>
  );
}
