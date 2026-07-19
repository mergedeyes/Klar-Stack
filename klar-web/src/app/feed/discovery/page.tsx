import React, { useState, useEffect, useRef, useCallback } from "react";
import { ENV } from "../env.ts";

interface Post {
  id: string;
  content: string;
  created_at: string;
  username: string;
}

interface CursorData {
  time: string;
  id: string;
}

interface FeedResponse {
  data: Post[];
  next_cursor: CursorData | null;
}

export default function DiscoveryFeed() {
  const [posts, setPosts] = useState<Post[]>([]);
  const [cursor, setCursor] = useState<CursorData | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [hasMore, setHasMore] = useState(true);

  // Referenz auf das unsichtbare HTML-Element ganz unten im Feed
  const observerTarget = useRef<HTMLDivElement>(null);

  const fetchPosts = useCallback(async (currentCursor: CursorData | null) => {
    if (isLoading || !hasMore) return;
    setIsLoading(true);

    try {
      // Backend-URL aus den Vite-Umgebungsvariablen laden oder Fallback auf lokalen Port
      const API_URL = ENV.API_URL;
      const url = new URL(`${API_URL}/feed/discovery`);
      
      url.searchParams.append("limit", "15"); // Limit auf 15 Posts pro Request
      
      // Wenn wir einen Cursor haben, hängen wir ihn als URL-Parameter an
      if (currentCursor) {
        url.searchParams.append("cursor_time", currentCursor.time);
        url.searchParams.append("cursor_id", currentCursor.id);
      }

      const response = await fetch(url.toString(), {
        // Hier können bei Bedarf Auth-Header mitgesendet werden
        // headers: { Authorization: `Bearer ${token}` }
      });
      
      if (!response.ok) throw new Error("Fehler beim Laden des Feeds");

      const feedData: FeedResponse = await response.json();

      setPosts((prev) => [...prev, ...feedData.data]);
      
      // Den neuen Cursor setzen. Wenn null zurückkommt, sind wir am Ende.
      setCursor(feedData.next_cursor);
      setHasMore(feedData.next_cursor !== null);

    } catch (error) {
      console.error("Discovery error:", error);
    } finally {
      setIsLoading(false);
    }
  }, [isLoading, hasMore]); 

  useEffect(() => {
    // Wenn das Ziel-Element den sichtbaren Bereich betritt, feuern wir fetchPosts
    const observer = new IntersectionObserver(
      (entries) => {
        // isIntersecting ist true, wenn der Sentinel im Bild auftaucht
        if (entries[0].isIntersecting && hasMore && !isLoading) {
          fetchPosts(cursor);
        }
      },
      {
        // Lädt schon nach, wenn das Ende noch 300px entfernt ist (Vermeidet Ruckler)
        rootMargin: "300px", 
        threshold: 0.1,
      }
    );

    const currentTarget = observerTarget.current;
    if (currentTarget) {
      observer.observe(currentTarget);
    }

    return () => {
      if (currentTarget) {
        observer.unobserve(currentTarget);
      }
    };
  }, [fetchPosts, hasMore, isLoading, cursor]);

  return (
    <div className="w-full max-w-2xl mx-auto py-8 px-4 flex flex-col gap-6 h-full">
      
      <div className="mb-4">
        <h1 className="text-3xl font-bold text-gray-900 dark:text-white tracking-tight">
          Discovery
        </h1>
        <p className="text-base text-gray-500 dark:text-gray-400 mt-1">
          Entdecke, was aktuell auf Klar passiert.
        </p>
      </div>

      {/* Feed Container */}
      <div className="flex flex-col gap-5">
        {posts.map((post) => (
          <article 
            key={post.id} 
            className="p-5 bg-white dark:bg-gray-800 rounded-2xl shadow-sm border border-gray-100 dark:border-gray-700 transition-shadow hover:shadow-md"
          >
            <div className="flex items-center gap-3 mb-3">
              <div className="w-10 h-10 rounded-full bg-gradient-to-tr from-blue-500 to-indigo-500 flex items-center justify-center text-white font-bold text-lg">
                {post.username.charAt(0).toUpperCase()}
              </div>
              <div>
                <h3 className="font-semibold text-gray-900 dark:text-white">
                  @{post.username}
                </h3>
                <span className="text-xs text-gray-500 dark:text-gray-400">
                  {new Date(post.created_at).toLocaleDateString('de-DE', { 
                    day: '2-digit', month: 'short', hour: '2-digit', minute: '2-digit' 
                  })}
                </span>
              </div>
            </div>
            <p className="text-gray-800 dark:text-gray-200 whitespace-pre-wrap leading-relaxed text-[15px]">
              {post.content}
            </p>
          </article>
        ))}
      </div>

      {}
      <div ref={observerTarget} className="w-full h-16 flex items-center justify-center mt-4">
        {isLoading && (
          <div className="flex items-center gap-3 py-4 px-6 bg-white dark:bg-gray-800 rounded-full shadow-sm border border-gray-100 dark:border-gray-700">
            <div className="w-5 h-5 border-[3px] border-blue-500 border-t-transparent rounded-full animate-spin"></div>
            <span className="text-sm text-gray-600 dark:text-gray-300 font-medium">Lade Beiträge...</span>
          </div>
        )}
        {!hasMore && posts.length > 0 && (
          <p className="text-sm text-gray-400 font-medium py-4">Du hast das Ende des Feeds erreicht. 🏁</p>
        )}
      </div>

    </div>
  );
}