"use client";

import { useEffect, useState, useCallback } from "react";
import Link from "next/link";
import Image from "next/image";
import { Heart, MessageCircle } from "lucide-react";
import { posts as postsApi, comments as commentsApi, type Post, type MediaAsset } from "@/lib/api";
import { useAuth } from "@/lib/auth-context";
import EditedBadge from "@/components/EditedBadge";
import { getMediaUrl } from "@/lib/utils/media";
import { ENV } from '../env';

const API_URL = ENV.API_URL;

function timeAgo(dateStr: string): string {
  const diff = Date.now() - new Date(dateStr).getTime();
  const mins = Math.floor(diff / 60000);
  if (mins < 1) return "just now";
  if (mins < 60) return `${mins}m`;
  const hrs = Math.floor(mins / 60);
  if (hrs < 24) return `${hrs}h`;
  const days = Math.floor(hrs / 24);
  if (days < 7) return `${days}d`;
  return new Date(dateStr).toLocaleDateString();
}

interface PostCardProps {
  post: Post;
  liked?: boolean;
  likeCount?: number;
  onOpenModal: (post: Post) => void;
  onLikeChange?: (postId: string, liked: boolean, count: number) => void;
}

export default function PostCard({
  post,
  liked: likedProp,
  likeCount: likeCountProp,
  onOpenModal,
  onLikeChange,
}: PostCardProps) {
  const { user } = useAuth();
  const [liked, setLiked] = useState(likedProp ?? false);
  const [likeCount, setLikeCount] = useState(likeCountProp ?? 0);
  const [liking, setLiking] = useState(false);

  // Sync like state from parent (e.g. after modal interaction)
  useEffect(() => { if (likedProp !== undefined) setLiked(likedProp); }, [likedProp]);
  useEffect(() => { if (likeCountProp !== undefined) setLikeCount(likeCountProp); }, [likeCountProp]);

  useEffect(() => {
    postsApi.getLikes(post.id).then((res) => {
      setLiked(res.liked);
      setLikeCount(res.like_count);
    }).catch(() => {});
  }, [post.id]);

  const handleLike = useCallback(async (e: React.MouseEvent) => {
    e.stopPropagation();
    if (!user || liking) return;
    setLiking(true);
    const newLiked = !liked;
    const newCount = newLiked ? likeCount + 1 : likeCount - 1;
    setLiked(newLiked);
    setLikeCount(newCount);
    try {
      const res = await postsApi.toggleLike(post.id);
      setLiked(res.liked);
      setLikeCount(res.like_count);
      onLikeChange?.(post.id, res.liked, res.like_count);
    } catch {
      setLiked(liked);
      setLikeCount(likeCount);
    } finally {
      setLiking(false);
    }
  }, [user, liking, liked, likeCount, post.id, onLikeChange]);

  return (
    <article className="border-b border-border py-4">
      {/* Header */}
      <div className="mb-3 flex items-center gap-3">
        <Link href={`/users/${post.username}`} onClick={(e) => e.stopPropagation()}>
          <div className="relative flex h-9 w-9 items-center justify-center overflow-hidden rounded-full bg-muted text-sm font-semibold uppercase">
            {post.avatar_url ? (
              <Image
                src={getMediaUrl(post.avatar_url)}
                alt={post.username}
                fill
                className="object-cover"
                unoptimized
              />
            ) : (
              post.username[0]
            )}
          </div>
        </Link>
        <div className="flex flex-1 items-center gap-2">
          <Link
            href={`/users/${post.username}`}
            onClick={(e) => e.stopPropagation()}
            className="text-sm font-semibold hover:underline"
          >
            {post.username}
          </Link>
          <span className="text-xs text-muted-foreground">{timeAgo(post.created_at)}</span>
          {post.edited_at && (
            <>
              <span className="text-xs text-muted-foreground">·</span>
              <EditedBadge editedAt={post.edited_at} />
            </>
          )}
        </div>
      </div>

      {/* Media */}
      {post.full_url && (
        <div
          className="mb-3 cursor-pointer overflow-hidden rounded-lg bg-muted"
          onClick={() => onOpenModal(post)}
        >
          <Image
            src={getMediaUrl(post.full_url)}
            alt={post.caption ?? "Post image"}
            width={640}
            height={640}
            className="w-full object-cover"
          />
        </div>
      )}

      {/* Caption */}
      {post.caption && (
        <p
          className="mb-3 cursor-pointer text-sm leading-relaxed"
          onClick={() => onOpenModal(post)}
        >
          <span className="mr-2 font-semibold">{post.username}</span>
          {post.caption}
        </p>
      )}

      {/* Actions */}
      <div className="flex items-center gap-4">
        <button
          onClick={handleLike}
          disabled={!user || liking}
          className="flex items-center gap-1.5 text-sm text-muted-foreground transition-colors hover:text-foreground disabled:cursor-not-allowed"
          aria-label={liked ? "Unlike" : "Like"}
        >
          <Heart size={18} className={liked ? "fill-red-500 stroke-red-500" : ""} />
          {likeCount > 0 && <span>{likeCount}</span>}
        </button>

        <button
          onClick={() => onOpenModal(post)}
          className="flex items-center gap-1.5 text-sm text-muted-foreground transition-colors hover:text-foreground"
          aria-label="View comments"
        >
          <MessageCircle size={18} />
          {post.comment_count !== undefined && post.comment_count > 0 && (
            <span>{post.comment_count}</span>
          )}
        </button>
      </div>
    </article>
  );
}