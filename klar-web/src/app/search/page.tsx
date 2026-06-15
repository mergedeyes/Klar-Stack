"use client";

import { useEffect, useRef, useState, useCallback } from "react";
import { useRouter } from "next/navigation";
import { ArrowLeft, Search, UserCheck, UserPlus } from "lucide-react";
import { useAuth } from "@/lib/auth-context";
import { users as usersApi, follows, type User } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { getMediaUrl } from "@/lib/utils/media";

const API_URL = process.env.NEXT_PUBLIC_API_URL ?? "http://localhost:3000";

// ── User result card ──────────────────────────────────────────────────────────

function UserCard({
  user,
  isMe,
  onNavigate,
}: {
  user: User;
  isMe: boolean;
  onNavigate: (username: string) => void;
}) {
  const [following, setFollowing] = useState(false);
  const [loading, setLoading] = useState(false);

  const handleFollow = async (e: React.MouseEvent) => {
    e.stopPropagation();
    if (loading) return;
    setLoading(true);
    try {
      if (following) {
        await follows.unfollow(user.username);
        setFollowing(false);
      } else {
        await follows.follow(user.username);
        setFollowing(true);
      }
    } catch {
      // Silently ignore — user can retry
    } finally {
      setLoading(false);
    }
  };

  return (
    <div
      className="flex cursor-pointer items-center gap-3 rounded-lg px-2 py-3 transition-colors hover:bg-muted/50"
      onClick={() => onNavigate(user.username)}
    >
      {/* Avatar */}
      <div className="relative h-11 w-11 shrink-0 overflow-hidden rounded-full bg-muted">
        {user.avatar_url ? (
          // eslint-disable-next-line @next/next/no-img-element
          <img
            src={getMediaUrl(user.avatar_url)}
            alt={user.username}
            className="h-full w-full object-cover"
          />
        ) : (
          <span className="flex h-full w-full items-center justify-center text-sm font-semibold uppercase">
            {user.username[0]}
          </span>
        )}
      </div>

      {/* Info */}
      <div className="min-w-0 flex-1">
        <p className="truncate text-sm font-semibold">{user.username}</p>
        {user.display_name && (
          <p className="truncate text-xs text-muted-foreground">
            {user.display_name}
          </p>
        )}
      </div>

      {/* Follow button — only for other users */}
      {!isMe && (
        <Button
          size="sm"
          variant={following ? "outline" : "default"}
          onClick={handleFollow}
          disabled={loading}
          className="shrink-0"
        >
          {following ? (
            <>
              <UserCheck size={14} className="mr-1" />
              Following
            </>
          ) : (
            <>
              <UserPlus size={14} className="mr-1" />
              Follow
            </>
          )}
        </Button>
      )}
    </div>
  );
}

// ── Page ──────────────────────────────────────────────────────────────────────

export default function SearchPage() {
  const { user, loading: authLoading } = useAuth();
  const router = useRouter();

  const [query, setQuery] = useState("");
  const [results, setResults] = useState<User[]>([]);
  const [searching, setSearching] = useState(false);
  const [searched, setSearched] = useState(false);
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  // Redirect if not logged in
  useEffect(() => {
    if (!authLoading && !user) router.push("/login");
  }, [user, authLoading, router]);

  // Auto-focus input on mount
  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  const doSearch = useCallback(async (q: string) => {
    if (!q.trim()) {
      setResults([]);
      setSearched(false);
      return;
    }
    setSearching(true);
    try {
      const res = await usersApi.search(q.trim());
      setResults(res);
      setSearched(true);
    } catch {
      setResults([]);
    } finally {
      setSearching(false);
    }
  }, []);

  const handleChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const val = e.target.value;
    setQuery(val);
    if (debounceRef.current) clearTimeout(debounceRef.current);
    debounceRef.current = setTimeout(() => doSearch(val), 300);
  };

  const handleNavigate = (username: string) => {
    router.push(`/users/${username}`);
  };

  if (authLoading) return null;
  if (!user) return null;

  return (
    <div className="min-h-screen bg-background">
      {/* Header */}
      <header className="sticky top-0 z-10 border-b border-border bg-background/80 backdrop-blur">
        <div className="mx-auto flex h-14 max-w-lg items-center gap-3 px-4">
          <Button
            variant="ghost"
            size="icon"
            onClick={() => router.push("/feed")}
            aria-label="Back to feed"
          >
            <ArrowLeft size={20} />
          </Button>
          <div className="relative flex-1">
            <Search
              size={16}
              className="absolute left-3 top-1/2 -translate-y-1/2 text-muted-foreground"
            />
            <Input
              ref={inputRef}
              value={query}
              onChange={handleChange}
              placeholder="Search people…"
              className="pl-9"
              autoComplete="off"
              autoCorrect="off"
              autoCapitalize="none"
            />
          </div>
        </div>
      </header>

      {/* Results */}
      <main className="mx-auto max-w-lg px-4 py-2">
        {searching && (
          <div className="flex justify-center py-8">
            <div className="h-5 w-5 animate-spin rounded-full border-2 border-muted border-t-foreground" />
          </div>
        )}

        {!searching && searched && results.length === 0 && (
          <p className="py-8 text-center text-sm text-muted-foreground">
            No users found for &ldquo;{query}&rdquo;
          </p>
        )}

        {!searching && !searched && (
          <p className="py-8 text-center text-sm text-muted-foreground">
            Search for people by username or name
          </p>
        )}

        {!searching &&
          results.map((result) => (
            <UserCard
              key={result.id}
              user={result}
              isMe={result.id === user.id}
              onNavigate={handleNavigate}
            />
          ))}
      </main>
    </div>
  );
}