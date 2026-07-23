"use client";

import { createContext, useContext, useEffect, useState, useCallback, createElement } from "react";
import { useAuth } from "@/lib/auth-context";
import { notifications as notificationsApi, chatsApi, auth, tokens, type AppNotification } from "@/lib/api";
import { ENV } from '@/env';

const API_URL = ENV.API_URL;

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

  useEffect(() => {
    if (!user) return;

    notificationsApi.list()
      .then((data) => {
        setNotifications(data);
        setUnreadCount(data.filter(n => !n.is_read).length);
      })
      .catch(err => console.error("Notification fetch failed:", err));

    chatsApi.getUnreadCount()
      .then((data) => setChatUnreadCount(data.count))
      .catch(err => console.error("Chat unread count fetch failed:", err));
  }, [user]);

  useEffect(() => {
    if (!user) return;

    let cancelled = false;
    let eventSource: EventSource | null = null;
    let retryTimer: ReturnType<typeof setTimeout> | null = null;

    const connect = () => {
      if (cancelled) return;

      const accessToken = tokens.getAccess();
      const streamUrl = accessToken
        ? `${API_URL}/notifications/stream?token=${encodeURIComponent(accessToken)}`
        : `${API_URL}/notifications/stream`;

      eventSource = new EventSource(streamUrl, {
        withCredentials: true,
      });

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

      eventSource.onerror = () => {
        eventSource?.close();
        if (cancelled) return;

        auth.refresh(tokens.getRefresh())
          .then((res) => {
            tokens.set(res.access_token, res.refresh_token);
            if (!cancelled) {
              retryTimer = setTimeout(connect, 1000);
            }
          })
          .catch(() => {
            // Refresh failed — session's actually gone. Don't retry; a
            // real login will remount this provider via the `user` dep.
          });
      };
    };

    connect();

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
  };

  return createElement(NotificationsContext.Provider, { value }, children);
}

export function useNotifications() {
  const ctx = useContext(NotificationsContext);
  if (!ctx) throw new Error("useNotifications must be used inside <NotificationsProvider>");
  return ctx;
}
