import type { Metadata } from "next";
import { ENV } from "@/env";
import PostPageClient from "./PostPageClient";

// Link previews (WhatsApp, Signal, iMessage, Slack, ...) read the
// Open Graph tags in the page's <head>, so they're generated here, on the
// server, before the interactive page (PostPageClient) takes over.
//
// The post is fetched without a login: private accounts, hidden or deleted
// posts come back as errors, so only posts that are public anyway ever get
// a preview. A post behind a content warning gets its title only, no
// caption or image. The image is our permanent /posts/:id/preview-image
// address (see the backend), not a signed media URL that expires within
// hours.

type Props = { params: Promise<{ id: string }> };

interface PublicPost {
  username: string;
  caption: string | null;
  moderation_status: string;
}

interface PublicMedia {
  width: number;
  height: number;
}

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const PREVIEW_WIDTH = 640; // the "medium" image variant the preview uses

async function fetchJson<T>(path: string): Promise<T | null> {
  try {
    // no-store: a post made private or deleted must stop producing previews
    // right away, not after a cache expires.
    const res = await fetch(`${ENV.API_URL}${path}`, { cache: "no-store" });
    return res.ok ? ((await res.json()) as T) : null;
  } catch {
    return null;
  }
}

function excerpt(text: string | null, max = 160): string | null {
  const clean = text?.replace(/\s+/g, " ").trim();
  if (!clean) return null;
  return clean.length <= max ? clean : `${clean.slice(0, max - 1).trimEnd()}…`;
}

export async function generateMetadata({ params }: Props): Promise<Metadata> {
  const { id } = await params;
  // Never in search results; previews are for links people share.
  const robots = { index: false, follow: false };

  const post = UUID.test(id) ? await fetchJson<PublicPost>(`/posts/${id}`) : null;
  if (!post) return { title: "Klar", robots };

  const url = `${ENV.SITE_URL}/posts/${id}`;
  const title = `@${post.username} on Klar`;
  const warned = post.moderation_status !== "visible";
  const description = (!warned && excerpt(post.caption)) || "A post on Klar";

  const media = warned ? null : await fetchJson<PublicMedia[]>(`/posts/${id}/media`);
  const first = media?.[0];
  const images = first
    ? [{
        url: `${ENV.API_URL}/posts/${id}/preview-image`,
        width: PREVIEW_WIDTH,
        height: Math.round((PREVIEW_WIDTH * first.height) / Math.max(first.width, 1)),
        alt: `Photo by @${post.username}`,
      }]
    : undefined;

  return {
    title: `Post by @${post.username} · Klar`,
    description,
    robots,
    alternates: { canonical: url },
    openGraph: { type: "article", siteName: "Klar", url, title, description, images },
    twitter: { card: images ? "summary_large_image" : "summary", title, description, images },
  };
}

export default function PostPage() {
  return <PostPageClient />;
}
