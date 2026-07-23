"use client";

import { useEffect, useRef, useState } from "react";
import { useRouter } from "next/navigation";
import Link from "next/link";
import {
  LogOut,
  Plus,
  Search,
  Settings,
  User,
  Bell,
  MessageCircle,
  Compass,
} from "lucide-react";
import { useAuth } from "@/lib/auth-context";
import { Button } from "@/components/ui/button";
import { useNotifications } from "@/hooks/use-notifications";
import CreatePostModal from "@/components/CreatePostModal";
import { getMediaUrl } from "@/lib/utils/media";
import { followRequestsApi, type AppNotification } from "@/lib/api";

export type TopNavSection = "feed" | "discovery" | "chats" | "search" | "profile";

interface TopNavProps {
  /** Which section is currently active, for icon highlighting. */
  active?: TopNavSection;
  /** Called after a post is created via the "+" modal, e.g. to refresh a feed list. */
  onPostCreated?: () => void;
}

/** Human-readable text for each notification type. 'message' never
 * reaches this dropdown (see use-notifications.ts), so it's not listed. */
function notificationText(typeName: string): string {
  switch (typeName) {
    case "post_like": return " liked your post";
    case "comment": return " commented on your post";
    case "comment_like": return " liked your comment";
    case "follow": return " started following you";
    case "follow_request": return " wants to follow you";
    case "follow_accepted": return " accepted your follow request";
    default: return " interacted with you";
  }
}

/** Where clicking a notification should go: the post for like/comment
 * types, the actor's profile for a follow (or an accepted request). A
 * follow_request itself isn't a link target -- it has its own
 * accept/decline buttons instead (see below). */
function notificationHref(n: AppNotification): string {
  if (n.type_name === "follow" || n.type_name === "follow_accepted") return `/users/${n.actor.username}`;
  if (n.post_id) return `/posts/${n.post_id}`;
  return "#";
}

/** Small preview thumbnail for a notification row — the actor's avatar
 * (with a default letter-fallback) for anything with no post involved
 * (follow, follow_request, follow_accepted), or the post's first image
 * (if it has one) for like/comment types. */
function NotificationPreview({ n }: { n: AppNotification }) {
  const noPostInvolved =
    n.type_name === "follow" || n.type_name === "follow_request" || n.type_name === "follow_accepted";

  if (noPostInvolved) {
    return (
      <div className="flex h-9 w-9 shrink-0 items-center justify-center overflow-hidden rounded-full bg-muted text-xs font-semibold uppercase">
        {n.actor.avatar_url ? (
          <img
            src={getMediaUrl(n.actor.avatar_url)}
            alt={n.actor.username}
            className="h-full w-full object-cover"
          />
        ) : (
          n.actor.username.charAt(0)
        )}
      </div>
    );
  }

  return (
    <div className="h-9 w-9 shrink-0 overflow-hidden rounded bg-muted">
      {n.post_thumb_url && (
        <img
          src={getMediaUrl(n.post_thumb_url)}
          alt=""
          className="h-full w-full object-cover"
        />
      )}
    </div>
  );
}

/**
 * Shared top bar for the app's primary destinations (Feed, Discovery, ...).
 * Not used on sub-pages like Settings or Search results, which use their
 * own back-button header instead.
 */
export default function TopNav({ active, onPostCreated }: TopNavProps) {
  const { user, logout } = useAuth();
  const router = useRouter();

  const { notifications, unreadCount, markAllAsRead, chatUnreadCount } = useNotifications();
  const [showNotifications, setShowNotifications] = useState(false);
  const [showCreate, setShowCreate] = useState(false);
  const dropdownRef = useRef<HTMLDivElement>(null);

  // Follow-request notifications the user has just acted on -- hidden
  // locally rather than mutating the shared notifications list (which
  // lives in the app-wide provider), since once accepted/declined they're
  // no longer actionable and showing stale Accept/Decline buttons would
  // be confusing.
  const [handledRequestIds, setHandledRequestIds] = useState<Set<string>>(new Set());
  const [requestActionLoading, setRequestActionLoading] = useState<string | null>(null);

  useOutsideClick(dropdownRef, showNotifications, () => setShowNotifications(false));

  const handleLogout = async () => {
    await logout();
    router.push("/login");
  };

  const handleRequestAction = async (n: AppNotification, accept: boolean) => {
    setRequestActionLoading(n.id);
    try {
      if (accept) {
        await followRequestsApi.accept(n.actor.username);
      } else {
        await followRequestsApi.reject(n.actor.username);
      }
      setHandledRequestIds((prev) => new Set(prev).add(n.id));
    } catch (err) {
      console.error("Failed to respond to follow request", err);
    } finally {
      setRequestActionLoading(null);
    }
  };

  // Every icon defaults to muted (grey); only the icon matching the
  // page's active section gets highlighted to the foreground color.
  const iconClass = (section?: TopNavSection) =>
    section && active === section ? "text-foreground bg-muted" : "text-muted-foreground";

  const visibleNotifications = notifications.filter((n) => !handledRequestIds.has(n.id));

  return (
    <>
      <header className="sticky top-0 z-10 border-b border-border bg-background/80 backdrop-blur">
        <div className="mx-auto flex h-14 max-w-3xl items-center justify-between px-4">
        <Link href="/feed" className="text-lg font-bold tracking-tight hover:opacity-80 transition-opacity">
          Klar
        </Link>
        <div className="flex items-center gap-2">
          <Button
            variant="ghost"
            size="icon"
            className={iconClass("discovery")}
            onClick={() => router.push("/feed/discovery")}
            aria-label="Discovery"
          >
            <Compass size={20} />
          </Button>
          <Button
            variant="ghost"
            size="icon"
            className={iconClass("search")}
            onClick={() => router.push("/search")}
            aria-label="Search"
          >
            <Search size={20} />
          </Button>
          <Button
            variant="ghost"
            size="icon"
            className={iconClass()}
            onClick={() => setShowCreate(true)}
            aria-label="New post"
          >
            <Plus size={20} />
          </Button>

          <div className="relative" ref={dropdownRef}>
            <Button
              variant="ghost"
              size="icon"
              className={iconClass()}
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
              <div className="absolute right-0 mt-2 w-80 rounded-md border border-border bg-background shadow-lg">
                <div className="p-3 text-sm font-semibold border-b border-border">Notifications</div>
                <div className="max-h-80 overflow-y-auto">
                  {visibleNotifications.length === 0 ? (
                    <div className="p-4 text-center text-sm text-muted-foreground">No notifications yet</div>
                  ) : (
                    visibleNotifications.map((n) =>
                      n.type_name === "follow_request" ? (
                        // Own row layout (not a whole-row Link) so the
                        // Accept/Decline buttons can sit as siblings next
                        // to the clickable name, rather than nested
                        // inside a link.
                        <div
                          key={n.id}
                          className={`flex items-center gap-3 p-3 text-sm border-b border-border last:border-0 ${!n.is_read ? "bg-muted/50" : ""}`}
                        >
                          <Link
                            href={`/users/${n.actor.username}`}
                            onClick={() => setShowNotifications(false)}
                            className="flex flex-1 items-center gap-3 min-w-0 hover:opacity-80"
                          >
                            <NotificationPreview n={n} />
                            <span className="min-w-0 truncate">
                              <span className="font-semibold">{n.actor.username}</span>
                              {notificationText(n.type_name)}
                            </span>
                          </Link>
                          <div className="flex shrink-0 gap-1">
                            <button
                              onClick={() => handleRequestAction(n, true)}
                              disabled={requestActionLoading === n.id}
                              className="rounded bg-primary px-2 py-1 text-xs font-medium text-primary-foreground disabled:opacity-50"
                            >
                              Accept
                            </button>
                            <button
                              onClick={() => handleRequestAction(n, false)}
                              disabled={requestActionLoading === n.id}
                              className="rounded border border-border px-2 py-1 text-xs font-medium disabled:opacity-50"
                            >
                              Decline
                            </button>
                          </div>
                        </div>
                      ) : (
                        <Link
                          key={n.id}
                          href={notificationHref(n)}
                          onClick={() => setShowNotifications(false)}
                          className={`flex items-center gap-3 p-3 text-sm border-b border-border last:border-0 hover:bg-muted/70 ${!n.is_read ? "bg-muted/50" : ""}`}
                        >
                          <NotificationPreview n={n} />
                          <span className="min-w-0">
                            <span className="font-semibold">{n.actor.username}</span>
                            {notificationText(n.type_name)}
                          </span>
                        </Link>
                      )
                    )
                  )}
                </div>
              </div>
            )}
          </div>

          <div className="relative">
            <Button
              variant="ghost"
              size="icon"
              className={iconClass("chats")}
              onClick={() => router.push("/chats")}
              aria-label="Chats"
            >
              <MessageCircle size={20} />
            </Button>
            {chatUnreadCount > 0 && (
              <span className="pointer-events-none absolute right-2 top-2 flex h-2 w-2 rounded-full bg-red-500" />
            )}
          </div>

          <Button
            variant="ghost"
            size="icon"
            className={iconClass("profile")}
            onClick={() => user && router.push(`/users/${user.username}`)}
            aria-label="Profile"
          >
            <User size={20} />
          </Button>
          <Button
            variant="ghost"
            size="icon"
            className={iconClass()}
            onClick={() => router.push("/settings")}
            aria-label="Settings"
          >
            <Settings size={20} />
          </Button>
          <Button
            variant="ghost"
            size="icon"
            className={iconClass()}
            onClick={handleLogout}
            aria-label="Log out"
          >
            <LogOut size={20} />
          </Button>
        </div>
      </div>

      </header>

      {showCreate && (
        <CreatePostModal
          onClose={() => setShowCreate(false)}
          onCreated={() => {
            setShowCreate(false);
            onPostCreated?.();
          }}
        />
      )}
    </>
  );
}

/** Closes a dropdown when a click lands outside the given ref, only while `active` is true. */
function useOutsideClick(
  ref: React.RefObject<HTMLElement | null>,
  active: boolean,
  onOutside: () => void
) {
  const savedRef = useRef(onOutside);
  savedRef.current = onOutside;

  useEffect(() => {
    if (!active) return;
    function handleClickOutside(event: MouseEvent) {
      if (ref.current && !ref.current.contains(event.target as Node)) {
        savedRef.current();
      }
    }
    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, [active, ref]);
}
