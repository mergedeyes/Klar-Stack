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
    default: return " interacted with you";
  }
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

  useOutsideClick(dropdownRef, showNotifications, () => setShowNotifications(false));

  const handleLogout = async () => {
    await logout();
    router.push("/login");
  };

  const iconClass = (section: TopNavSection) =>
    active === section ? "text-foreground bg-muted" : "text-muted-foreground";

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
          <Button variant="ghost" size="icon" onClick={() => setShowCreate(true)} aria-label="New post">
            <Plus size={20} />
          </Button>

          <div className="relative" ref={dropdownRef}>
            <Button
              variant="ghost"
              size="icon"
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
              <div className="absolute right-0 mt-2 w-72 rounded-md border border-border bg-background shadow-lg">
                <div className="p-3 text-sm font-semibold border-b border-border">Notifications</div>
                <div className="max-h-64 overflow-y-auto">
                  {notifications.length === 0 ? (
                    <div className="p-4 text-center text-sm text-muted-foreground">No notifications yet</div>
                  ) : (
                    notifications.map((n) => (
                      <div
                        key={n.id}
                        className={`p-3 text-sm border-b border-border last:border-0 ${!n.is_read ? "bg-muted/50" : ""}`}
                      >
                        <span className="font-semibold">{n.actor.username}</span>
                        {notificationText(n.type_name)}
                      </div>
                    ))
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
          <Button variant="ghost" size="icon" onClick={() => router.push("/settings")} aria-label="Settings">
            <Settings size={20} />
          </Button>
          <Button variant="ghost" size="icon" onClick={handleLogout} aria-label="Log out">
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
