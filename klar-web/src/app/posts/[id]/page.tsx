"use client";

import { useEffect, useState } from "react";
import { useParams, useRouter } from "next/navigation";
import { posts as postsApi, type Post } from "@/lib/api";
import PostModal from "@/components/PostModal";

/**
 * Permalink for a single post — opens it directly in PostModal instead of
 * only being reachable by clicking through a feed. Used by notification
 * links (see TopNav), but also just a real shareable URL for any post.
 */
export default function PostPage() {
  const params = useParams<{ id: string }>();
  const router = useRouter();
  const [post, setPost] = useState<Post | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    postsApi.get(params.id)
      .then(setPost)
      .catch((err) => setError(err instanceof Error ? err.message : "Post not found"));
  }, [params.id]);

  if (error) {
    return (
      <div className="flex min-h-screen flex-col items-center justify-center gap-2 bg-background text-sm text-muted-foreground">
        <p>{error}</p>
        <button onClick={() => router.push("/feed")} className="underline">
          Back to feed
        </button>
      </div>
    );
  }

  if (!post) {
    return (
      <div className="flex min-h-screen items-center justify-center bg-background">
        <div className="h-6 w-6 animate-spin rounded-full border-2 border-muted border-t-foreground" />
      </div>
    );
  }

  return (
    <PostModal
      post={post}
      onClose={() => router.push("/feed")}
      onDeleted={() => router.push("/feed")}
    />
  );
}
