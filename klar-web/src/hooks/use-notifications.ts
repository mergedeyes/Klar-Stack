"use client";

import { createContext, useContext, useEffect, useState, useCallback, createElement } from "react";
import { useAuth } from "@/lib/auth-context";
import { notifications as notificationsApi, chatsApi, getAccessToken, type AppNotification } from "@/lib/api";
import { ENV } from '@/env';

const API_URL = ENV.API_URL;

// Reconnect backoff for the notification stream.
const RETRY_MIN_MS = 1_000;
const RETRY_MAX_MS = 60_000;

export type { AppNotification };

interface LastMessageEvent {
  senderId: string;
  at: number;
}

interface NotificationsContextValue {
  notifications: AppNotification[];
  unreadCount: number;
  markAllAsRead: () => void;
  chatUnreadCount: number;
  /** Bumped every time a live "message" SSE event arrives (see
   * chats.rs's send_message). ChatWindow watches this and, if
   * senderId matches whoever it's currently showing a conversation
   * with, refetches — this is what makes an open chat update live
   * instead of only on reload. */
  lastMessageEvent: LastMessageEvent | null;
  /** Re-fetches the true unread-message count from the server. Call this
   * right after marking a conversation read — the badge otherwise only
   * updates on the next full mount of this provider (e.g. a page reload),
   * since nothing else tells it a conversation was just cleared. */
  refreshChatUnreadCount: () => void;
}

const NotificationsContext = createContext<NotificationsContextValue | null>(null);

/**
 * Single, app-wide SSE connection + notification state, provided once at
 * the root (see layout.tsx) rather than per-component. Previously this
 * lived entirely inside TopNav's own hook, which meant any page that
 * doesn't render TopNav (like /chats) had no SSE connection running at
 * all -- that's why chat messages only ever showed up after a reload.
 *
 * Written with createElement instead of JSX so this stays a plain .ts
 * file rather than .tsx -- avoids a same-basename .ts/.tsx module
 * collision, since this file previously held a plain (non-provider) hook.
 */
export function NotificationsProvider({ children }: { children: React.ReactNode }) {
  const { user } = useAuth();
  const [notifications, setNotifications] = useState<AppNotification[]>([]);
  const [unreadCount, setUnreadCount] = useState(0);
  const [chatUnreadCount, setChatUnreadCount] = useState(0);
  const [lastMessageEvent, setLastMessageEvent] = useState<LastMessageEvent | null>(null);

  const refreshChatUnreadCount = useCallback(() => {
    if (!user) return;
    chatsApi.getUnreadCount()
      .then((data) => setChatUnreadCount(data.count))
      .catch(err => console.error("Chat unread count refresh failed:", err));
  }, [user]);

  useEffect(() => {
    if (!user) return;

    notificationsApi.list()
      .then((data) => {
        setNotifications(data);
        setUnreadCount(data.filter(n => !n.is_read).length);
      })
      .catch(err => console.error("Notification fetch failed:", err));

    refreshChatUnreadCount();
  }, [user, refreshChatUnreadCount]);

  useEffect(() => {
    if (!user) return;

    let cancelled = false;
    let eventSource: EventSource | null = null;
    let retryTimer: ReturnType<typeof setTimeout> | null = null;
    let retryDelay = RETRY_MIN_MS;

    const connect = async () => {
      if (cancelled) return;

      // A fresh single-use ticket per connection attempt. Fetched through
      // request(), so an expiring access token is refreshed via the same
      // shared refresh as every other call (refreshing separately here used
      // to race it with the same single-use refresh token and log the user
      // out when it lost).
      if (!(await getAccessToken())) return; // session is gone; a new login remounts this
      let ticket: string;
      try {
        ({ ticket } = await notificationsApi.streamTicket());
      } catch (err) {
        console.error("SSE ticket request failed:", err);
        if (cancelled) return;
        retryTimer = setTimeout(connect, retryDelay);
        retryDelay = Math.min(retryDelay * 2, RETRY_MAX_MS);
        return;
      }
      if (cancelled) return;

      eventSource = new EventSource(
        `${API_URL}/notifications/stream?ticket=${encodeURIComponent(ticket)}`,
        { withCredentials: true }
      );

      eventSource.onopen = () => {
        retryDelay = RETRY_MIN_MS;
      };

      eventSource.onmessage = (event) => {
        try {
          const incoming: AppNotification = JSON.parse(event.data);

          if (incoming.type_name === "message") {
            setChatUnreadCount(prev => prev + 1);
            setLastMessageEvent({ senderId: incoming.actor.id, at: Date.now() });
            return;
          }

          setNotifications(prev => [incoming, ...prev]);
          setUnreadCount(prev => prev + 1);
        } catch (err) {
          console.error("Failed to parse SSE message", err);
        }
      };

      // EventSource can't tell us *why* it failed (used/expired ticket,
      // network blip, server restart), so always reconnect with backoff:
      // connect() fetches a new ticket each time.
      eventSource.onerror = () => {
        eventSource?.close();
        if (cancelled) return;
        retryTimer = setTimeout(connect, retryDelay);
        retryDelay = Math.min(retryDelay * 2, RETRY_MAX_MS);
      };
    };

    void connect();

    return () => {
      cancelled = true;
      if (retryTimer) clearTimeout(retryTimer);
      eventSource?.close();
    };
  }, [user]);

  const markAllAsRead = useCallback(() => {
    if (unreadCount === 0 || !user) return;

    setUnreadCount(0);
    setNotifications(prev => prev.map(n => ({ ...n, is_read: true })));

    notificationsApi.markRead().catch(err =>
      console.error("Failed to mark notifications as read", err)
    );
  }, [unreadCount, user]);

  const value: NotificationsContextValue = {
    notifications,
    unreadCount,
    markAllAsRead,
    chatUnreadCount,
    lastMessageEvent,
    refreshChatUnreadCount,
  };

  return createElement(NotificationsContext.Provider, { value }, children);
}

export function useNotifications() {
  const ctx = useContext(NotificationsContext);
  if (!ctx) throw new Error("useNotifications must be used inside <NotificationsProvider>");
  return ctx;
}
