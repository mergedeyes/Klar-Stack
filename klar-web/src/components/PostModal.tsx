"use client";

import { useEffect, useRef, useState, useCallback, useMemo } from "react";
import Image from "next/image";
import Link from "next/link";
import { Heart, Send, Trash2, X, Flag, ShieldAlert } from "lucide-react";
import { useAuth } from "@/lib/auth-context";
import {
  posts as postsApi,
  comments as commentsApi,
  type Post,
  type MediaAsset,
  type Comment,
} from "@/lib/api";
import { Button } from "@/components/ui/button";
import EditedBadge from "@/components/EditedBadge";
import ReportModal from "@/components/ReportModal";
import { useRouter } from "next/navigation";
import { getMediaUrl } from "@/lib/utils/media";
import { ENV } from '@/env';

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

// ── @mention renderer ─────────────────────────────────────────────────────────

function CommentText({ body }: { body: string }) {
  const parts = body.split(/(@\w+)/g);
  return (
    <>
      {parts.map((part, i) =>
        /^@\w+$/.test(part) ? (
          <Link
            key={i}
            href={`/users/${part.slice(1)}`}
            className="font-semibold text-foreground hover:underline"
            onClick={(e) => e.stopPropagation()}
          >
            {part}
          </Link>
        ) : (
          <span key={i}>{part}</span>
        )
      )}
    </>
  );
}

// ── Avatar ────────────────────────────────────────────────────────────────────

function Avatar({ username, avatarUrl, size = 8 }: { username: string; avatarUrl: string | null; size?: number }) {
  const cls = `relative flex shrink-0 items-center justify-center overflow-hidden rounded-full bg-muted font-semibold uppercase`;
  const dim = `h-${size} w-${size}`;
  const textSize = size <= 8 ? "text-xs" : "text-sm";
  return (
    <div className={`${cls} ${dim} ${textSize}`}>
      {avatarUrl ? (
        <Image src={getMediaUrl(avatarUrl)} alt={username} fill className="object-cover" unoptimized />
      ) : (
        username[0]
      )}
    </div>
  );
}

// ── Comment row ───────────────────────────────────────────────────────────────

function CommentRow({
  comment,
  currentUsername,
  postUsername,
  postId,
  depth,
  onDelete,
  onEdited,
  onReply,
}: {
  comment: Comment;
  currentUsername: string | undefined;
  postUsername: string;
  postId: string;
  depth: number;
  onDelete: (id: string) => void;
  onEdited: (updated: Comment) => void;
  onReply: (username: string, commentId: string) => void;
}) {
  const [editing, setEditing] = useState(false);
  const [editBody, setEditBody] = useState(comment.body);
  const [saving, setSaving] = useState(false);
  const [liked, setLiked] = useState(comment.liked);
  const [likeCount, setLikeCount] = useState(comment.like_count);
  const [liking, setLiking] = useState(false);
  const editInputRef = useRef<HTMLTextAreaElement>(null);

  const isAuthor = currentUsername === comment.username;
  const canDelete = isAuthor || currentUsername === postUsername;

  useEffect(() => {
    if (editing) editInputRef.current?.focus();
  }, [editing]);

  const handleLike = async () => {
    if (!currentUsername || liking) return;
    setLiking(true);
    const wasLiked = liked;
    setLiked(!wasLiked);
    setLikeCount((c) => (wasLiked ? c - 1 : c + 1));
    try {
      const res = await commentsApi.toggleLike(postId, comment.id);
      setLiked(res.liked);
      setLikeCount(res.like_count);
    } catch {
      setLiked(wasLiked);
      setLikeCount((c) => (wasLiked ? c + 1 : c - 1));
    } finally {
      setLiking(false);
    }
  };

  const handleSave = async () => {
    if (!editBody.trim() || saving) return;
    setSaving(true);
    try {
      const updated = await commentsApi.edit(postId, comment.id, editBody.trim());
      onEdited(updated);
      setEditing(false);
    } catch {
      // silently ignore
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className={depth > 0 ? "ml-8 mt-2" : "mt-3"}>
      <div className="flex gap-2.5">
        <Avatar username={comment.username} avatarUrl={comment.avatar_url} size={8} />
        <div className="flex-1 min-w-0">
          {editing ? (
            <div className="space-y-1.5">
              <textarea
                ref={editInputRef}
                value={editBody}
                onChange={(e) => setEditBody(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); handleSave(); }
                  if (e.key === "Escape") { setEditBody(comment.body); setEditing(false); }
                }}
                rows={2}
                maxLength={2000}
                className="w-full resize-none rounded border border-border bg-transparent px-2 py-1 text-sm outline-none focus:border-foreground"
                disabled={saving}
              />
              <div className="flex gap-3">
                <button onClick={handleSave} disabled={!editBody.trim() || saving} className="text-xs font-semibold hover:underline disabled:opacity-50">Save</button>
                <button onClick={() => { setEditBody(comment.body); setEditing(false); }} className="text-xs text-muted-foreground hover:underline">Cancel</button>
              </div>
            </div>
          ) : (
            <>
              <p className="text-sm leading-snug">
                <Link href={`/users/${comment.username}`} className="mr-1.5 font-semibold hover:underline" onClick={(e) => e.stopPropagation()}>
                  {comment.username}
                </Link>
                <CommentText body={comment.body} />
              </p>
              <div className="mt-1 flex items-center gap-3">
                <span className="text-xs text-muted-foreground">{timeAgo(comment.created_at)}</span>
                {comment.edited_at && <EditedBadge editedAt={comment.edited_at} />}
                {likeCount > 0 && (
                  <span className="text-xs text-muted-foreground">{likeCount} like{likeCount !== 1 ? "s" : ""}</span>
                )}
                {currentUsername && (
                  <button
                    onClick={() => onReply(comment.username, comment.id)}
                    className="text-xs text-muted-foreground hover:text-foreground"
                  >
                    Reply
                  </button>
                )}
                {isAuthor && (
                  <button onClick={() => setEditing(true)} className="text-xs text-muted-foreground hover:text-foreground">Edit</button>
                )}
                {canDelete && (
                  <button onClick={() => onDelete(comment.id)} className="text-xs text-muted-foreground hover:text-destructive">Delete</button>
                )}
              </div>
            </>
          )}
        </div>

        {/* Like button */}
        {!editing && (
          <button
            onClick={handleLike}
            disabled={!currentUsername || liking}
            className="mt-0.5 shrink-0 self-start disabled:cursor-not-allowed"
            aria-label={liked ? "Unlike comment" : "Like comment"}
          >
            <Heart
              size={14}
              className={liked ? "fill-red-500 stroke-red-500" : "text-muted-foreground hover:text-foreground"}
            />
          </button>
        )}
      </div>
    </div>
  );
}

// ── Modal ─────────────────────────────────────────────────────────────────────

interface PostModalProps {
  post: Post;
  onClose: () => void;
  onLikeChange?: (postId: string, liked: boolean, count: number) => void;
  onDeleted?: (postId: string) => void;
}

export default function PostModal({ post, onClose, onLikeChange, onDeleted }: PostModalProps) {
  const { user } = useAuth();
  const router = useRouter();
  const [deleting, setDeleting] = useState(false);
  const [showReportModal, setShowReportModal] = useState(false);
  // Reported (violence/self-harm/sexual-content severity) posts are
  // gated behind this until the viewer explicitly clicks through --
  // never auto-set true for the post's own owner, who should always be
  // able to see their own content (just with a heads-up banner instead).
  const [viewAnyway, setViewAnyway] = useState(false);

  const [liked, setLiked] = useState(false);
  const [likeCount, setLikeCount] = useState(0);
  const [liking, setLiking] = useState(false);
  const [media, setMedia] = useState<MediaAsset[]>([]);
  const [allComments, setAllComments] = useState<Comment[]>([]);
  const [commentBody, setCommentBody] = useState("");
  const [replyTo, setReplyTo] = useState<{ username: string; commentId: string } | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [loadingComments, setLoadingComments] = useState(true);
  const commentsEndRef = useRef<HTMLDivElement>(null);
  const commentInputRef = useRef<HTMLInputElement>(null);

  const isOwner = user?.username === post.username;
  const isFlagged = post.moderation_status === "flagged";
  const showInterstitial = isFlagged && !isOwner && !viewAnyway;

  useEffect(() => {
    // Skip fetching likes/media/comments while the interstitial is up --
    // no reason to load content the viewer hasn't agreed to see yet.
    if (showInterstitial) return;
    postsApi.getLikes(post.id).then((res) => { setLiked(res.liked); setLikeCount(res.like_count); }).catch(() => {});
    postsApi.media(post.id).then((res) => { setMedia(res.filter((a) => a.medium_url)); }).catch(() => {});
    commentsApi.list(post.id).then(setAllComments).catch(() => {}).finally(() => setLoadingComments(false));
  }, [post.id, showInterstitial]);

  useEffect(() => {
    const handler = (e: KeyboardEvent) => { if (e.key === "Escape") onClose(); };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [onClose]);

  useEffect(() => {
    document.body.style.overflow = "hidden";
    return () => { document.body.style.overflow = ""; };
  }, []);

  // When replying, pre-fill @username and focus input
  const handleSetReply = useCallback((username: string, commentId: string) => {
    setReplyTo({ username, commentId });
    setCommentBody(`@${username} `);
    setTimeout(() => commentInputRef.current?.focus(), 50);
  }, []);

  const clearReply = useCallback(() => {
    setReplyTo(null);
    setCommentBody("");
  }, []);

  // Build threaded comment tree
  const { topLevel, repliesMap } = useMemo(() => {
    const top = allComments.filter((c) => !c.parent_comment_id);
    const map: Record<string, Comment[]> = {};
    allComments.forEach((c) => {
      if (c.parent_comment_id) {
        if (!map[c.parent_comment_id]) map[c.parent_comment_id] = [];
        map[c.parent_comment_id].push(c);
      }
    });
    return { topLevel: top, repliesMap: map };
  }, [allComments]);

  const handleLike = useCallback(async () => {
    if (!user || liking) return;
    setLiking(true);
    const newLiked = !liked;
    setLiked(newLiked);
    setLikeCount((c) => newLiked ? c + 1 : c - 1);
    try {
      const res = await postsApi.toggleLike(post.id);
      setLiked(res.liked);
      setLikeCount(res.like_count);
      onLikeChange?.(post.id, res.liked, res.like_count);
    } catch {
      setLiked(liked);
      setLikeCount((c) => newLiked ? c - 1 : c + 1);
    } finally {
      setLiking(false);
    }
  }, [user, liking, liked, post.id, onLikeChange]);

  const handleSubmitComment = async () => {
    if (!commentBody.trim() || submitting || !user) return;
    setSubmitting(true);
    try {
      const newComment = await commentsApi.create(post.id, commentBody.trim(), replyTo?.commentId);
      setAllComments((prev) => [...prev, newComment]);
      setCommentBody("");
      setReplyTo(null);
      setTimeout(() => commentsEndRef.current?.scrollIntoView({ behavior: "smooth" }), 50);
    } catch {
      // silently ignore
    } finally {
      setSubmitting(false);
    }
  };

  const handleDelete = async (commentId: string) => {
    try {
      await commentsApi.delete(post.id, commentId);
      // Also remove all replies to deleted comment
      setAllComments((prev) => prev.filter((c) => c.id !== commentId && c.parent_comment_id !== commentId));
    } catch {}
  };

  const handleDeletePost = async () => {
    // 1. Guard clauses to prevent double-clicks or unauthorized attempts
    if (!user || deleting) return;

    // 2. Add a safety check so users don't accidentally delete their memories
    if (!window.confirm("Are you sure you want to delete this post? This cannot be undone.")) {
      return;
    }

    setDeleting(true);
    
    try {
      // 3. Make the call to the Rust backend
      await postsApi.delete(post.id);

      // 4. Tell the parent component (ProfilePage) to remove the post from the grid
      if (onDeleted) {
        onDeleted(post.id);
      }

      // 5. Close the modal
      onClose();
      
    } catch (error) {
      console.error("Failed to delete post:", error);
      alert("Failed to delete post. Please try again.");
      setDeleting(false); // Only reset if it fails, since success unmounts the modal anyway
    }
  };

  const handleEdited = (updated: Comment) => {
    setAllComments((prev) => prev.map((c) => c.id === updated.id ? updated : c));
  };

  const renderComments = (comments: Comment[], depth = 0) =>
    comments.map((comment) => (
      <div key={comment.id}>
        <CommentRow
          comment={comment}
          currentUsername={user?.username}
          postUsername={post.username}
          postId={post.id}
          depth={depth}
          onDelete={handleDelete}
          onEdited={handleEdited}
          onReply={handleSetReply}
        />
        {repliesMap[comment.id] && renderComments(repliesMap[comment.id], depth + 1)}
      </div>
    ));

  const firstImage = media[0];

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4"
      onClick={(e) => { if (e.target === e.currentTarget) onClose(); }}
    >
      <div className="relative flex max-h-[90vh] w-full max-w-4xl flex-col overflow-hidden rounded-xl bg-background shadow-2xl md:flex-row">
        {user?.username === post.username && !showInterstitial && (
          <button
            onClick={handleDeletePost}
            disabled={deleting}
            className="absolute right-20 top-3 z-10 rounded-full bg-background/80 p-1.5 text-muted-foreground backdrop-blur hover:text-destructive disabled:opacity-50"
            aria-label="Delete post"
          >
            <Trash2 size={18} />
          </button>
        )}
        {user && !isOwner && !showInterstitial && (
          <button
            onClick={() => setShowReportModal(true)}
            className="absolute right-12 top-3 z-10 rounded-full bg-background/80 p-1.5 text-muted-foreground backdrop-blur hover:text-foreground"
            aria-label="Report post"
          >
            <Flag size={18} />
          </button>
        )}
        <button onClick={onClose} className="absolute right-3 top-3 z-10 rounded-full bg-background/80 p-1.5 text-muted-foreground backdrop-blur hover:text-foreground" aria-label="Close">
          <X size={18} />
        </button>

        {showInterstitial ? (
          <div className="flex w-full flex-col items-center justify-center gap-4 px-8 py-16 text-center">
            <ShieldAlert size={40} className="text-muted-foreground" />
            <div>
              <p className="font-semibold">This post may violate our guidelines</p>
              <p className="mt-1 text-sm text-muted-foreground">
                It&apos;s been reported and is pending review.
              </p>
            </div>
            <div className="flex gap-2">
              <Button variant="outline" onClick={onClose}>Go back</Button>
              <Button variant="secondary" onClick={() => setViewAnyway(true)}>View anyway</Button>
            </div>
          </div>
        ) : (
          <>
            {isFlagged && isOwner && (
              <div className="absolute left-3 top-3 z-10 rounded-md bg-background/90 px-2 py-1 text-xs text-muted-foreground backdrop-blur">
                Pending review — only visible to you and people who click through
              </div>
            )}

            {/* image panel */}
            {firstImage?.medium_url && (
              <div className="flex w-full shrink-0 items-center justify-center bg-black md:w-[58%]">
                <Image
                  src={getMediaUrl(firstImage.medium_url)}
                  alt={post.caption ?? "Post image"}
                  width={firstImage.width ?? 1080}
                  height={firstImage.height ?? 1080}
                  className="h-auto w-full object-contain max-h-[50vh] md:max-h-[90vh]"
                  unoptimized
                />
              </div>
            )}

            {/* Detail panel */}
            <div className={`flex min-h-0 flex-1 flex-col ${firstImage?.medium_url ? "" : "w-full"}`}>

              {/* Author header */}
              <div className="flex items-center gap-3 border-b border-border px-4 py-3">
                <Link href={`/users/${post.username}`} onClick={onClose}>
                  <Avatar username={post.username} avatarUrl={post.avatar_url ?? null} size={9} />
                </Link>
                <div>
                  <Link href={`/users/${post.username}`} onClick={onClose} className="text-sm font-semibold hover:underline">
                    {post.username}
                  </Link>
                  <p className="flex items-center gap-1.5 text-xs text-muted-foreground">
                    {timeAgo(post.created_at)}
                    {post.edited_at && <><span>·</span><EditedBadge editedAt={post.edited_at} /></>}
                  </p>
                </div>
              </div>

              {/* Scrollable content */}
              <div className="flex-1 overflow-y-auto px-4 py-3">
                {/* Caption */}
                {post.caption && (
                  <div className="mb-3 flex gap-2.5">
                    <Avatar username={post.username} avatarUrl={post.avatar_url ?? null} size={8} />
                    <p className="text-sm leading-snug">
                      <Link href={`/users/${post.username}`} onClick={onClose} className="mr-1.5 font-semibold hover:underline">
                        {post.username}
                      </Link>
                      <CommentText body={post.caption} />
                    </p>
                  </div>
                )}

                {/* Comments */}
                {loadingComments && (
                  <div className="flex justify-center py-4">
                    <div className="h-4 w-4 animate-spin rounded-full border-2 border-muted border-t-foreground" />
                  </div>
                )}
                {!loadingComments && allComments.length === 0 && (
                  <p className="py-4 text-center text-xs text-muted-foreground">No comments yet. Be the first!</p>
                )}
                {renderComments(topLevel)}
                <div ref={commentsEndRef} />
              </div>

              {/* Like bar */}
              <div className="border-t border-border px-4 py-3">
                <button onClick={handleLike} disabled={!user || liking} className="flex items-center gap-2 text-sm disabled:cursor-not-allowed">
                  <Heart size={20} className={liked ? "fill-red-500 stroke-red-500" : "text-muted-foreground"} />
                  <span className={liked ? "font-semibold" : "text-muted-foreground"}>
                    {likeCount > 0 ? `${likeCount} like${likeCount === 1 ? "" : "s"}` : "Be the first to like this"}
                  </span>
                </button>
              </div>

              {/* Comment input */}
              {user && (
                <div className="border-t border-border px-4 py-3">
                  {replyTo && (
                    <div className="mb-2 flex items-center justify-between rounded bg-muted px-2 py-1">
                      <span className="text-xs text-muted-foreground">
                        Replying to <span className="font-semibold text-foreground">@{replyTo.username}</span>
                      </span>
                      <button onClick={clearReply} className="text-xs text-muted-foreground hover:text-foreground">✕</button>
                    </div>
                  )}
                  <div className="flex items-center gap-2">
                    <input
                      ref={commentInputRef}
                      value={commentBody}
                      onChange={(e) => setCommentBody(e.target.value)}
                      onKeyDown={(e) => {
                        if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); handleSubmitComment(); }
                      }}
                      placeholder={replyTo ? `Reply to @${replyTo.username}…` : "Add a comment…"}
                      maxLength={2000}
                      className="flex-1 bg-transparent text-sm outline-none placeholder:text-muted-foreground"
                      disabled={submitting}
                    />
                    <Button size="sm" variant="ghost" onClick={handleSubmitComment} disabled={!commentBody.trim() || submitting} aria-label="Post comment">
                      <Send size={16} />
                    </Button>
                  </div>
                </div>
              )}
            </div>
          </>
        )}
      </div>

      {showReportModal && (
        <ReportModal
          targetType="post"
          targetId={post.id}
          onClose={() => setShowReportModal(false)}
        />
      )}
    </div>
  );
}
