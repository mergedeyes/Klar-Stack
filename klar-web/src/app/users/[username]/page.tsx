"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import { useParams, useRouter } from "next/navigation";
import Link from "next/link";
import { Grid3X3, Lock, Flag, ShieldAlert, UserX } from "lucide-react";
import Image from "next/image";
import {
  users as usersApi,
  follows,
  followRequestsApi,
  blocks as blocksApi,
  posts as postsApi,
  cursorAfter,
  type User,
  type ProfileStats,
  type Post,
  type PostCursor,
} from "@/lib/api";
import { useAuth } from "@/lib/auth-context";
import { getMediaUrl } from "@/lib/utils/media";
import { Button } from "@/components/ui/button";
import PostModal from "@/components/PostModal";
import ReportModal from "@/components/ReportModal";
import { SmartBackButton } from '@/components/SmartBackButton';
import UserListModal from "@/components/UserListModal";
import TopNav from "@/components/TopNav";

// ── Post grid cell ────────────────────────────────────────────────────────────

function GridCell({
  post,
  onClick,
}: {
  post: Post;
  onClick: () => void;
}) {
  const thumb = post.medium_url ? getMediaUrl(post.medium_url) : null;

  return (
    <button
      onClick={onClick}
      className="group relative aspect-square w-full overflow-hidden bg-muted focus:outline-none"
    >
      {thumb ? (
        <Image
          src={thumb}
          alt={post.caption ?? "Post"}
          fill
          className="object-cover transition-opacity group-hover:opacity-80"
          unoptimized
        />
      ) : (
        <div className="flex h-full w-full items-center justify-center p-2">
          <p className="line-clamp-4 text-center text-xs text-muted-foreground">
            {post.caption ?? ""}
          </p>
        </div>
      )}
    </button>
  );
}

// ── Page ──────────────────────────────────────────────────────────────────────

// A multiple of three so every page fills whole grid rows.
const POSTS_PAGE_SIZE = 30;

export default function ProfilePage() {
  const { username } = useParams<{ username: string }>();
  const { user: me, loading: authLoading } = useAuth();
  const router = useRouter();

  const [profile, setProfile] = useState<User | null>(null);
  const [stats, setStats] = useState<ProfileStats | null>(null);
  const [userPosts, setUserPosts] = useState<Post[]>([]);
  const [postsBlocked, setPostsBlocked] = useState(false);
  const [hasMorePosts, setHasMorePosts] = useState(false);
  const [loadingMorePosts, setLoadingMorePosts] = useState(false);
  const [loadMoreFailed, setLoadMoreFailed] = useState(false);
  const postsCursorRef = useRef<PostCursor | undefined>(undefined);
  // Bumped whenever the grid is reloaded from the first page or the
  // profile changes, so a request still in flight for the old list drops
  // its result instead of mixing it into the new one.
  const postsGenRef = useRef(0);
  const [followLoading, setFollowLoading] = useState(false);
  const [requestActionLoading, setRequestActionLoading] = useState(false);
  const [isBlocked, setIsBlocked] = useState(false);
  const [blockLoading, setBlockLoading] = useState(false);
  // The username whose profile fetch last settled. Loading is derived from
  // it, so navigating to another profile shows the spinner again without
  // setting state from the effect.
  const [loadedFor, setLoadedFor] = useState<string | null>(null);
  const loading = loadedFor !== username;
  const [activePost, setActivePost] = useState<Post | null>(null);
  const [showReportModal, setShowReportModal] = useState(false);
  // The name whose profile couldn't be loaded: deleted, suspended, or a
  // typo.
  const [notFoundFor, setNotFoundFor] = useState<string | null>(null);
  const notFound = notFoundFor === username;

  const [followers, setFollowers] = useState<User[]>([]);
  const [following, setFollowing] = useState<User[]>([]);
  const [modalType, setModalType] = useState<"followers" | "following" | null>(null);

  const isMe = !!me && !!profile && me.id === profile.id;
  // "requested" / "following" / "not_following" / "self" / undefined
  // (undefined only while loading, or if the profile fetch predates this
  // field existing -- treated as not_following).
  const relationship = profile?.viewer_relationship;
  const isFollowing = relationship === "following";
  const isRequested = relationship === "requested";

  // Posts are visible to: the owner, an accepted follower, or anyone if
  // the account isn't private. Computed from the profile fetch alone, so
  // the posts endpoint doesn't even need to be called (and 403) for a
  // private account you can't see into.
  const canSeePosts = !!profile && (!profile.is_private || isMe || isFollowing);
  // Posts fetched while the account was visible stay in state after e.g.
  // an unfollow of a private account; they just aren't shown.
  const visiblePosts = canSeePosts ? userPosts : [];

  const refreshPosts = useCallback(() => {
    if (!username || !canSeePosts) return;
    const gen = ++postsGenRef.current;
    postsCursorRef.current = undefined;
    postsApi.userPosts(username, undefined, POSTS_PAGE_SIZE)
      .then((page) => {
        if (gen !== postsGenRef.current) return;
        const full = page.length >= POSTS_PAGE_SIZE;
        setUserPosts(page);
        setPostsBlocked(false);
        setHasMorePosts(full);
        setLoadMoreFailed(false);
        postsCursorRef.current = full ? cursorAfter(page) : undefined;
      })
      .catch(() => {
        if (gen === postsGenRef.current) setPostsBlocked(true);
      });
  }, [username, canSeePosts]);

  const loadMorePosts = useCallback(async () => {
    const cursor = postsCursorRef.current;
    if (!username || !canSeePosts || !cursor) return;
    const gen = postsGenRef.current;
    setLoadingMorePosts(true);
    try {
      const page = await postsApi.userPosts(username, cursor, POSTS_PAGE_SIZE);
      if (gen !== postsGenRef.current) return;
      setUserPosts((prev) => [...prev, ...page]);
      const full = page.length >= POSTS_PAGE_SIZE;
      setHasMorePosts(full);
      postsCursorRef.current = full ? cursorAfter(page) : undefined;
    } catch {
      // Keep what's already shown and stop auto-loading; otherwise the
      // re-created observer would retry in a tight loop while the sentinel
      // stays on screen. The user retries explicitly.
      if (gen === postsGenRef.current) setLoadMoreFailed(true);
    } finally {
      setLoadingMorePosts(false);
    }
  }, [username, canSeePosts]);

  // A callback ref rather than an effect because the sentinel only mounts
  // once the profile has loaded. Re-created whenever the list changes, so
  // the observer's initial callback fetches again if the sentinel is still
  // on screen (e.g. a tall window that one page doesn't fill).
  const postsSentinelRef = useCallback((node: HTMLDivElement | null) => {
    if (!node || userPosts.length === 0) return;
    if (!hasMorePosts || loadingMorePosts || loadMoreFailed) return;
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries[0].isIntersecting) {
          observer.disconnect();
          loadMorePosts();
        }
      },
      { rootMargin: "400px" },
    );
    observer.observe(node);
    return () => observer.disconnect();
  }, [hasMorePosts, loadingMorePosts, loadMoreFailed, loadMorePosts, userPosts]);

  useEffect(() => {
    if (!username) return;
    let cancelled = false;
    // The previous profile's cursor must not be used against this one.
    postsGenRef.current++;
    postsCursorRef.current = undefined;

    usersApi.get(username)
      .then((profileData) => {
        if (cancelled) return;
        setProfile(profileData);
      })
      .catch(() => {
        if (!cancelled) setNotFoundFor(username);
      })
      .finally(() => {
        if (!cancelled) setLoadedFor(username);
      });

    usersApi.stats(username).then((s) => { if (!cancelled) setStats(s); }).catch(() => {});

    return () => { cancelled = true; };
  }, [username, router]);

  // Fetch posts once we know whether we're allowed to see them (depends
  // on the profile fetch above having resolved viewer_relationship).
  useEffect(() => {
    if (!profile || !canSeePosts) return;
    refreshPosts();
  }, [profile, canSeePosts, refreshPosts]);

  useEffect(() => {
    if (!isMe || !username) return;
    let cancelled = false;
    
    Promise.all([
      follows.followers(username),
      follows.following(username),
    ]).then(([followersData, followingData]) => {
      if (cancelled) return;
      setFollowers(followersData);
      setFollowing(followingData);
    }).catch(console.error);

    return () => { cancelled = true; };
  }, [isMe, username]);

  const handleBlockToggle = useCallback(async () => {
    if (!me || blockLoading) return;
    setBlockLoading(true);
    const wasBlocked = isBlocked;
    setIsBlocked(!wasBlocked);
    try {
      if (wasBlocked) {
        await blocksApi.unblock(username);
      } else {
        await blocksApi.block(username);
      }
    } catch {
      setIsBlocked(wasBlocked);
    } finally {
      setBlockLoading(false);
    }
  }, [me, blockLoading, isBlocked, username]);

  // One button handles all three states: not_following -> follow (which
  // may itself land on "following" or "requested" depending on whether
  // the account is private), requested -> cancel the request, following
  // -> unfollow. All via the same follow()/unfollow() calls; the backend
  // decides what "follow" actually means for this target.
  const handleFollowToggle = useCallback(async () => {
    if (!me || !profile || followLoading) return;
    setFollowLoading(true);

    const wasFollowing = isFollowing;
    const wasRequested = isRequested;

    try {
      if (wasFollowing || wasRequested) {
        await follows.unfollow(username);
        setProfile((p) => p ? { ...p, viewer_relationship: "not_following" } : p);
        if (wasFollowing) {
          setStats((prev) => prev ? { ...prev, followers: prev.followers - 1 } : prev);
        }
      } else {
        const res = await follows.follow(username);
        setProfile((p) => p ? { ...p, viewer_relationship: res.status } : p);
        if (res.status === "following") {
          setStats((prev) => prev ? { ...prev, followers: prev.followers + 1 } : prev);
        }
      }
    } catch {
      // Leave state as-is on failure -- no optimistic update happened above.
    } finally {
      setFollowLoading(false);
    }
  }, [me, profile, followLoading, isFollowing, isRequested, username]);

  // This profile (someone else) has sent *me* a pending follow request --
  // accept/decline it right here, same as in the notification dropdown.
  const handleRequestResponse = useCallback(async (accept: boolean) => {
    if (!profile || requestActionLoading) return;
    setRequestActionLoading(true);
    try {
      if (accept) {
        await followRequestsApi.accept(profile.username);
      } else {
        await followRequestsApi.reject(profile.username);
      }
      setProfile((p) => p ? { ...p, incoming_follow_request: false } : p);
    } catch {
      // leave as-is on failure
    } finally {
      setRequestActionLoading(false);
    }
  }, [profile, requestActionLoading]);

  if (loading || authLoading) {
    return (
      <div className="flex-1 bg-background">
        <div className="mx-auto max-w-3xl animate-pulse px-4 pt-6">
          <div className="mb-6 flex items-center gap-5">
            <div className="h-24 w-24 md:h-32 md:w-32 rounded-full bg-muted" />
            <div className="flex-1 space-y-2">
              <div className="h-6 w-48 rounded bg-muted" />
              <div className="h-4 w-32 rounded bg-muted" />
            </div>
          </div>
          <div className="grid grid-cols-3 gap-1">
            {Array.from({ length: 9 }).map((_, i) => (
              <div key={i} className="aspect-square bg-muted" />
            ))}
          </div>
        </div>
      </div>
    );
  }

  // Said so, instead of quietly landing on the feed: a mistyped name, or
  // an account that was deleted or is suspended.
  if (notFound) {
    return (
      <div className="flex-1 bg-background">
        <header className="sticky top-0 z-20 border-b border-border bg-background/80 backdrop-blur">
          <div className="mx-auto flex h-14 max-w-3xl items-center gap-3 px-4">
            <SmartBackButton aria-label="Back" />
          </div>
        </header>
        <main className="mx-auto max-w-3xl px-4 py-16 text-center">
          <UserX size={36} className="mx-auto mb-3 text-muted-foreground" />
          <p className="font-semibold">Profil nicht gefunden</p>
          <p className="mt-1 text-sm text-muted-foreground">
            There&rsquo;s no profile called @{username} — the name may be mistyped, or the account was deleted or is
            suspended.
          </p>
        </main>
      </div>
    );
  }

  if (!profile) return null;

  return (
    <div className="flex-1 bg-background pb-12">
      {/* Own profile uses the shared primary nav; viewing someone else uses a back-button header */}
      {isMe ? (
        <TopNav active="profile" onPostCreated={refreshPosts} />
      ) : (
        <header className="sticky top-0 z-20 border-b border-border bg-background/80 backdrop-blur">
          <div className="mx-auto flex h-14 max-w-3xl items-center gap-3 px-4">
            <SmartBackButton aria-label="Back" />
            <span className="font-semibold flex-1">{profile.username}</span>
            {!isMe && me && (
              <button
                onClick={() => setShowReportModal(true)}
                aria-label="Report user"
                className="text-muted-foreground hover:text-foreground"
              >
                <Flag size={18} />
              </button>
            )}
            {/* The account's standing: measures, or parts of the profile
                removed (with or without a report). */}
            {!isMe && me?.is_admin && (
              <Link
                href={`/admin/standing/${profile.username}`}
                aria-label="Moderate user"
                className="text-muted-foreground hover:text-foreground"
              >
                <ShieldAlert size={18} />
              </Link>
            )}
          </div>
        </header>
      )}

      <main className="mx-auto max-w-3xl">
        {/* Profile info */}
        <div className="flex flex-col sm:flex-row gap-6 items-start sm:items-center px-4 py-8">
          
          {/* Avatar */}
          <div className="relative h-24 w-24 sm:h-32 sm:w-32 shrink-0 overflow-hidden rounded-full bg-muted">
            {profile.avatar_url ? (
              <Image
                src={getMediaUrl(profile.avatar_url)}
                alt={profile.username}
                fill
                className="object-cover"
                unoptimized
              />
            ) : (
              <span className="flex h-full w-full items-center justify-center text-3xl font-bold uppercase">
                {profile.username[0]}
              </span>
            )}
          </div>

          <div className="flex-1 w-full">
            {/* Name */}
            <div className="mb-4">
              <h1 className="flex items-center gap-2 text-2xl font-bold">
                {profile.display_name || profile.username}
                {profile.is_private && (
                  <Lock size={16} className="text-muted-foreground" aria-label="Private account" />
                )}
              </h1>
              <p className="text-muted-foreground">@{profile.username}</p>
            </div>

            {/* Stats */}
            <div className="flex gap-8 mb-4">
              <div className="flex flex-col items-center">
                <span className="text-lg font-bold">{stats?.posts ?? 0}</span>
                <span className="text-sm text-muted-foreground">Posts</span>
              </div>
              <button 
                onClick={() => isMe && setModalType("followers")}
                disabled={!isMe}
                className={`flex flex-col items-center focus:outline-none ${isMe ? 'hover:opacity-70 transition-opacity' : 'cursor-default'}`}
              >
                <span className="text-lg font-bold">{stats?.followers ?? 0}</span>
                <span className="text-sm text-muted-foreground">Followers</span>
              </button>
              <button 
                onClick={() => isMe && setModalType("following")}
                disabled={!isMe}
                className={`flex flex-col items-center focus:outline-none ${isMe ? 'hover:opacity-70 transition-opacity' : 'cursor-default'}`}
              >
                <span className="text-lg font-bold">{stats?.following ?? 0}</span>
                <span className="text-sm text-muted-foreground">Following</span>
              </button>
            </div>

            {/* Bio */}
            {profile.bio && (
              <p className="whitespace-pre-wrap text-sm mb-6">{profile.bio}</p>
            )}

            {/* This person wants to follow me -- accept/decline right here,
                same as in the notification dropdown. */}
            {!isMe && profile.incoming_follow_request && (
              <div className="mb-3 flex items-center justify-between rounded-lg border border-border bg-muted/40 px-3 py-2">
                <span className="text-sm">
                  <strong>{profile.username}</strong> wants to follow you
                </span>
                <div className="flex gap-2">
                  <Button size="sm" onClick={() => handleRequestResponse(true)} disabled={requestActionLoading}>
                    Accept
                  </Button>
                  <Button size="sm" variant="outline" onClick={() => handleRequestResponse(false)} disabled={requestActionLoading}>
                    Decline
                  </Button>
                </div>
              </div>
            )}

            {/* Action buttons */}
            {isMe ? (
              <div className="flex flex-wrap gap-2">
                <Button
                  variant="outline"
                  className="w-full sm:w-auto"
                  onClick={() => router.push("/settings/profile")}
                >
                  Edit profile
                </Button>
                {profile.is_private && (
                  <Button
                    variant="outline"
                    className="w-full sm:w-auto"
                    onClick={() => router.push("/follow-requests")}
                  >
                    Follow requests
                  </Button>
                )}
              </div>
            ) : me ? (
              <div className="flex flex-wrap gap-2">
                <Button
                  variant={isFollowing || isRequested ? "outline" : "default"}
                  className="flex-1 sm:flex-none min-w-[120px]"
                  onClick={handleFollowToggle}
                  disabled={followLoading || isBlocked}
                >
                  {isFollowing ? "Following" : isRequested ? "Requested" : "Follow"}
                </Button>
                
                <Button
                  variant="secondary"
                  className="flex-1 sm:flex-none min-w-[120px]"
                  onClick={() => router.push(`/chats?uid=${profile.id}&un=${encodeURIComponent(profile.username)}&av=${encodeURIComponent(profile.avatar_url || '')}`)}
                >
                  Message
                </Button>

                <Button
                  variant="destructive"
                  className="shrink-0"
                  onClick={handleBlockToggle}
                  disabled={blockLoading}
                >
                  {isBlocked ? "Unblock" : "Block"}
                </Button>
              </div>
            ) : null}
          </div>
        </div>

        {/* Posts grid */}
        <div className="border-t border-border mt-4">
          {!canSeePosts ? (
            <div className="py-16 text-center">
              <Lock size={32} className="mx-auto mb-3 text-muted-foreground" />
              <p className="text-sm font-semibold">This account is private</p>
              <p className="mt-1 text-sm text-muted-foreground">
                Follow this account to see their posts.
              </p>
            </div>
          ) : postsBlocked ? (
            <div className="py-16 text-center">
              <p className="text-sm text-muted-foreground">Couldn&apos;t load posts</p>
            </div>
          ) : visiblePosts.length === 0 ? (
            <div className="py-16 text-center">
              <Grid3X3 size={32} className="mx-auto mb-3 text-muted-foreground" />
              <p className="text-sm text-muted-foreground">No posts yet</p>
            </div>
          ) : (
            <div className="grid grid-cols-3 gap-1">
              {visiblePosts.map((post) => (
                <GridCell
                  key={post.id}
                  post={post}
                  onClick={() => setActivePost(post)}
                />
              ))}
            </div>
          )}
          {canSeePosts && !postsBlocked && hasMorePosts && (
            <div ref={postsSentinelRef} className="flex justify-center py-6">
              {loadMoreFailed ? (
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => { setLoadMoreFailed(false); loadMorePosts(); }}
                >
                  Couldn&apos;t load more posts — retry
                </Button>
              ) : loadingMorePosts && (
                <div className="h-5 w-5 animate-spin rounded-full border-2 border-muted border-t-foreground" />
              )}
            </div>
          )}
        </div>
      </main>

      {activePost && (
        <PostModal
          post={activePost}
          onClose={() => setActivePost(null)}
        />
      )}

      {/* Follow/Following Modal */}
      {modalType && (
        <UserListModal 
          title={modalType === "followers" ? "Followers" : "Following"} 
          users={modalType === "followers" ? followers : following} 
          onClose={() => setModalType(null)} 
        />
      )}

      {showReportModal && (
        <ReportModal
          targetType="user"
          targetId={profile.id}
          authorUsername={profile.username}
          onClose={() => setShowReportModal(false)}
        />
      )}
    </div>
  );
}
