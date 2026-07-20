"use client";

import { useCallback, useEffect, useState } from "react";
import { useParams, useRouter } from "next/navigation";
import { Grid3X3 } from "lucide-react";
import Image from "next/image";
import {
  users as usersApi,
  follows,
  blocks as blocksApi,
  posts as postsApi,
  type User,
  type ProfileStats,
  type Post,
} from "@/lib/api";
import { useAuth } from "@/lib/auth-context";
import { getMediaUrl } from "@/lib/utils/media";
import { Button } from "@/components/ui/button";
import PostModal from "@/components/PostModal";
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

export default function ProfilePage() {
  const { username } = useParams<{ username: string }>();
  const { user: me, loading: authLoading } = useAuth();
  const router = useRouter();

  const [profile, setProfile] = useState<User | null>(null);
  const [stats, setStats] = useState<ProfileStats | null>(null);
  const [userPosts, setUserPosts] = useState<Post[]>([]);
  const [isFollowing, setIsFollowing] = useState(false);
  const [followLoading, setFollowLoading] = useState(false);
  const [isBlocked, setIsBlocked] = useState(false);
  const [blockLoading, setBlockLoading] = useState(false);
  const [loading, setLoading] = useState(true);
  const [activePost, setActivePost] = useState<Post | null>(null);

  const [followers, setFollowers] = useState<User[]>([]);
  const [following, setFollowing] = useState<User[]>([]);
  const [modalType, setModalType] = useState<"followers" | "following" | null>(null);

  const isMe = !!me && !!profile && me.id === profile.id;

  const refreshPosts = useCallback(() => {
    if (!username) return;
    postsApi.userPosts(username, undefined, 50).then(setUserPosts);
  }, [username]);

  useEffect(() => {
    if (!username) return;
    let cancelled = false;

    setLoading(true);
    Promise.all([
      usersApi.get(username),
      usersApi.stats(username),
      postsApi.userPosts(username, undefined, 50),
    ])
      .then(([profileData, statsData, postsData]) => {
        if (cancelled) return;
        setProfile(profileData);
        setStats(statsData);
        setUserPosts(postsData);
      })
      .catch(() => {
        if (!cancelled) router.push("/feed");
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });

    return () => { cancelled = true; };
  }, [username, router]);

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

  useEffect(() => {
    if (!me || isMe || !username) return;
    follows.followers(username).then((followerList) => {
      setIsFollowing(followerList.some((f) => f.id === me.id));
    }).catch(() => {});
  }, [me, isMe, username]);

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
        setIsFollowing(false);
      }
    } catch {
      setIsBlocked(wasBlocked);
    } finally {
      setBlockLoading(false);
    }
  }, [me, blockLoading, isBlocked, username]);

  const handleFollowToggle = useCallback(async () => {
    if (!me || followLoading) return;
    setFollowLoading(true);
    const wasFollowing = isFollowing;
    setIsFollowing(!wasFollowing);
    setStats((prev) =>
      prev
        ? { ...prev, followers: prev.followers + (wasFollowing ? -1 : 1) }
        : prev
    );
    try {
      if (wasFollowing) {
        await follows.unfollow(username);
      } else {
        await follows.follow(username);
      }
    } catch {
      setIsFollowing(wasFollowing);
      setStats((prev) =>
        prev
          ? { ...prev, followers: prev.followers + (wasFollowing ? 1 : -1) }
          : prev
      );
    } finally {
      setFollowLoading(false);
    }
  }, [me, followLoading, isFollowing, username]);

  if (loading || authLoading) {
    return (
      <div className="min-h-screen bg-background">
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

  if (!profile) return null;

  return (
    <div className="min-h-screen bg-background pb-12">
      {/* Own profile uses the shared primary nav; viewing someone else uses a back-button header */}
      {isMe ? (
        <TopNav active="profile" onPostCreated={refreshPosts} />
      ) : (
        <header className="sticky top-0 z-20 border-b border-border bg-background/80 backdrop-blur">
          <div className="mx-auto flex h-14 max-w-3xl items-center gap-3 px-4">
            <SmartBackButton aria-label="Back" />
            <span className="font-semibold">{profile.username}</span>
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
              <h1 className="text-2xl font-bold">{profile.display_name || profile.username}</h1>
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

            {/* Action buttons */}
            {isMe ? (
              <Button
                variant="outline"
                className="w-full sm:w-auto"
                onClick={() => router.push("/settings/profile")}
              >
                Edit profile
              </Button>
            ) : me ? (
              <div className="flex flex-wrap gap-2">
                <Button
                  variant={isFollowing ? "outline" : "default"}
                  className="flex-1 sm:flex-none min-w-[120px]"
                  onClick={handleFollowToggle}
                  disabled={followLoading || isBlocked}
                >
                  {isFollowing ? "Following" : "Follow"}
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
          {userPosts.length === 0 ? (
            <div className="py-16 text-center">
              <Grid3X3 size={32} className="mx-auto mb-3 text-muted-foreground" />
              <p className="text-sm text-muted-foreground">No posts yet</p>
            </div>
          ) : (
            <div className="grid grid-cols-3 gap-1">
              {userPosts.map((post) => (
                <GridCell
                  key={post.id}
                  post={post}
                  onClick={() => setActivePost(post)}
                />
              ))}
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
    </div>
  );
}
