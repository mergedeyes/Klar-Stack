"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import { useParams, useRouter } from "next/navigation";
import { ImageOff } from "lucide-react";
import { posts as postsApi, type Post } from "@/lib/api";
import { useAuth } from "@/lib/auth-context";
import { useSmartBack } from "@/hooks/use-smart-back";
import { SmartBackButton } from "@/components/SmartBackButton";
import PostView from "@/components/PostView";

/**
 * The post page: where a shared link or a reload of /posts/:id lands. Opened
 * from inside the app, the same post shows as a modal over the feed or
 * profile instead (PostModal); both render PostView.
 */
export default function PostPage() {
  const params = useParams<{ id: string }>();
  const router = useRouter();
  const goBack = useSmartBack();
  const { user } = useAuth();
  const [post, setPost] = useState<Post | null>(null);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    postsApi.get(params.id)
      .then(setPost)
      .catch(() => setFailed(true));
  }, [params.id]);

  useEffect(() => {
    if (post) document.title = `Post by @${post.username} · Klar`;
  }, [post]);

  // From tablet width up the page fits the screen exactly (the card takes
  // the height between header and footer, its comments scroll inside);
  // on phones the page scrolls as a whole.
  return (
    <div data-fill-viewport-md className="flex min-h-0 flex-1 flex-col bg-background">
      <header className="sticky top-0 z-20 border-b border-border bg-background/80 backdrop-blur">
        <div className="mx-auto flex h-14 max-w-5xl items-center gap-3 px-4">
          <SmartBackButton aria-label="Back" />
          <span className="font-semibold">Post</span>
        </div>
      </header>

      <div className="md:flex md:min-h-0 md:flex-1 md:flex-col md:px-4 md:py-6">
        {failed ? (
          // Deleted, hidden, or from a private account the viewer doesn't
          // follow: the API answers the same way, so the page does too.
          <div className="flex flex-col items-center px-4 py-24 text-center">
            <ImageOff size={36} className="mb-3 text-muted-foreground" />
            <p className="font-semibold">This post isn&apos;t available</p>
            <p className="mt-1 max-w-xs text-sm text-muted-foreground">
              It may have been deleted, or it&apos;s from a private account you don&apos;t follow.
            </p>
            <Link href={user ? "/feed" : "/login"} className="mt-4 text-sm underline">
              {user ? "Back to your feed" : "Sign in"}
            </Link>
          </div>
        ) : !post ? (
          <div className="mx-auto w-full max-w-5xl animate-pulse md:flex md:min-h-0 md:flex-1 md:max-h-[56rem] md:overflow-hidden md:rounded-xl md:border md:border-border">
            <div className="aspect-square w-full bg-muted md:aspect-auto md:flex-1" />
            <div className="space-y-3 p-4 md:w-[26rem]">
              <div className="h-4 w-32 rounded bg-muted" />
              <div className="h-3 w-48 rounded bg-muted" />
            </div>
          </div>
        ) : (
          <PostView
            post={post}
            layout="page"
            onBack={goBack}
            afterDelete={() => router.replace(`/users/${post.username}`)}
          />
        )}
      </div>
    </div>
  );
}
