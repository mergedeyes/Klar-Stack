"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { LogOut, Plus, Search, Settings, User, Bell } from "lucide-react";
import { useAuth } from "@/lib/auth-context";
import { posts as postsApi, type Post } from "@/lib/api";
import PostCard from "@/components/PostCard";
import PostModal from "@/components/PostModal";
import CreatePostModal from "@/components/CreatePostModal";
import { Button } from "@/components/ui/button";
import Link from 'next/link';
import { useNotifications } from "@/hooks/use-notifications";

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
  const { user, loading: authLoading, logout } = useAuth();
  const router = useRouter();

  const [feedPosts, setFeedPosts] = useState<Post[]>([]);
  const [loading, setLoading] = useState(true);
  const [loadingMore, setLoadingMore] = useState(false);
  const [hasMore, setHasMore] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const cursorRef = useRef<string | undefined>(undefined);

  const [activePost, setActivePost] = useState<Post | null>(null);
  const [showCreate, setShowCreate] = useState(false);

  const { notifications, unreadCount, markAllAsRead } = useNotifications();
  const [showNotifications, setShowNotifications] = useState(false);
  
  // Ref for the dropdown to detect clicks outside
  const dropdownRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!authLoading && !user) router.push("/login");
  }, [user, authLoading, router]);

  // Click-outside listener for the notification dropdown
  useEffect(() => {
    function handleClickOutside(event: MouseEvent) {
      if (dropdownRef.current && !dropdownRef.current.contains(event.target as Node)) {
        setShowNotifications(false);
      }
    }

    if (showNotifications) {
      document.addEventListener("mousedown", handleClickOutside);
    }
    
    return () => {
      document.removeEventListener("mousedown", handleClickOutside);
    };
  }, [showNotifications]);

  useEffect(() => {
    if (authLoading || !user) return;

    let cancelled = false;
    cursorRef.current = undefined;
    setLoading(true);
    setError(null);
    setHasMore(true);

    postsApi.feed(undefined, 20)
      .then((page) => {
        if (cancelled) return;
        setFeedPosts(page);
        if (page.length < 20) {
          setHasMore(false);
        } else {
          cursorRef.current = page[page.length - 1].created_at;
        }
      })
      .catch((err) => {
        if (cancelled) return;
        setError(err instanceof Error ? err.message : "Failed to load feed");
      })
      .finally(() => {
        if (cancelled) return;
        setLoading(false);
      });

    return () => { cancelled = true; };
  }, [authLoading, user]);

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

  const handleLogout = async () => {
    await logout();
    router.push("/login");
  };

  const showSkeletons = authLoading || loading;

  return (
    <div className="min-h-screen bg-background">
      <header className="sticky top-0 z-10 border-b border-border bg-background/80 backdrop-blur">
        <div className="mx-auto flex h-14 max-w-lg items-center justify-between px-4">
          <Link href="/feed" className="text-lg font-bold tracking-tight hover:opacity-80 transition-opacity">
            Klar
          </Link>
          <div className="flex items-center gap-2">
            <Button variant="ghost" size="icon" onClick={() => router.push("/search")} aria-label="Search">
              <Search size={20} />
            </Button>
            <Button variant="ghost" size="icon" onClick={() => setShowCreate(true)} aria-label="New post">
              <Plus size={20} />
            </Button>

            {/* Notification container gets the ref */}
            <div className="relative" ref={dropdownRef}>
              <Button 
                variant="ghost" 
                size="icon" 
                onClick={() => {
                  setShowNotifications(!showNotifications);
                  if (!showNotifications) markAllAsRead();
                }}
                aria-label="Notifications"
              >
                <Bell size={20} />
                {unreadCount > 0 && (
                  <span className="absolute right-2 top-2 flex h-2 w-2 rounded-full bg-red-500" />
                )}
              </Button>

              {showNotifications && (
                <div className="absolute right-0 mt-2 w-72 rounded-md border border-border bg-background shadow-lg">
                  <div className="p-3 text-sm font-semibold border-b border-border">Notifications</div>
                  <div className="max-h-64 overflow-y-auto">
                    {notifications.length === 0 ? (
                      <div className="p-4 text-center text-sm text-muted-foreground">No notifications yet</div>
                    ) : (
                      notifications.map(n => (
                        <div key={n.id} className={`p-3 text-sm border-b border-border last:border-0 ${!n.is_read ? 'bg-muted/50' : ''}`}>
                          <span className="font-semibold">{n.actor.username}</span> 
                          {n.type_name === 'post_like' ? ' liked your post' : ' interacted with you'}
                        </div>
                      ))
                    )}
                  </div>
                </div>
              )}
            </div>

            <Button variant="ghost" size="icon" onClick={() => user && router.push(`/users/${user.username}`)} aria-label="Profile">
              <User size={20} />
            </Button>
            <Button variant="ghost" size="icon" onClick={() => router.push("/settings")} aria-label="Settings">
              <Settings size={20} />
            </Button>
            <Button variant="ghost" size="icon" onClick={handleLogout} aria-label="Log out">
              <LogOut size={20} />
            </Button>
          </div>
        </div>
      </header>

      <main className="mx-auto max-w-lg px-4">
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

      {showCreate && (
        <CreatePostModal
          onClose={() => setShowCreate(false)}
          onCreated={() => {
            setFeedPosts([]);
            setHasMore(true);
            cursorRef.current = undefined;
            setLoading(true);
            postsApi.feed(undefined, 20).then((page) => {
              setFeedPosts(page);
              if (page.length < 20) setHasMore(false);
              else cursorRef.current = page[page.length - 1].created_at;
            }).finally(() => setLoading(false));
          }}
        />
      )}

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