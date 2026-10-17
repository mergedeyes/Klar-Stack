"use client";

import { createContext, useContext, useEffect, useEffectEvent, useState, useCallback, createElement } from "react";
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
  /** When the app last caught up after missing live events: the stream
   * reconnected, or the app came back to the foreground (phones close the
   * connection in the background). Pages showing live data (an open chat,
   * the chat list) reload when it changes; 0 until the first catch-up. */
  resyncedAt: number;
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
  const [resyncedAt, setResyncedAt] = useState(0);

  const refreshChatUnreadCount = useCallback(() => {
    if (!user) return;
    chatsApi.getUnreadCount()
      .then((data) => setChatUnreadCount(data.count))
      .catch(err => console.error("Chat unread count refresh failed:", err));
  }, [user]);

  const loadNotifications = useCallback(() => {
    if (!user) return;
    notificationsApi.list()
      .then((data) => {
        setNotifications(data);
        setUnreadCount(data.filter(n => !n.is_read).length);
      })
      .catch(err => console.error("Notification fetch failed:", err));
  }, [user]);

  useEffect(() => {
    loadNotifications();
    refreshChatUnreadCount();
  }, [loadNotifications, refreshChatUnreadCount]);

  // Whatever arrived while the stream was down was never pushed, so it's
  // fetched instead. An effect event, so the stream below isn't torn down
  // and reopened whenever these callbacks change.
  const resync = useEffectEvent(() => {
    loadNotifications();
    refreshChatUnreadCount();
    setResyncedAt(Date.now());
  });

  useEffect(() => {
    if (!user) return;

    let cancelled = false;
    let eventSource: EventSource | null = null;
    let retryTimer: ReturnType<typeof setTimeout> | null = null;
    let retryDelay = RETRY_MIN_MS;
    // True while a connection is being set up (the ticket request), so a
    // return to the foreground doesn't start a second one.
    let connecting = false;
    // Set whenever the stream is down, so the next connection catches up
    // on what it didn't push meanwhile.
    let missedEvents = false;

    const connect = async () => {
      if (cancelled || connecting) return;
      connecting = true;
      try {
        await openStream();
      } finally {
        connecting = false;
      }
    };

    const openStream = async () => {
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
        missedEvents = true;
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
        if (missedEvents) {
          missedEvents = false;
          resync();
        }
      };

      eventSource.onmessage = (event) => {
        try {
          const incoming: AppNotification = JSON.parse(event.data);

          if (incoming.type_name === "message") {
            setChatUnreadCount(prev => prev + 1);
            if (incoming.actor) setLastMessageEvent({ senderId: incoming.actor.id, at: Date.now() });
            return;
          }
          // An edited or deleted message: the open chat refreshes, but
          // nothing new arrived, so the unread badge stays as it is.
          if (incoming.type_name === "message_changed") {
            if (incoming.actor) setLastMessageEvent({ senderId: incoming.actor.id, at: Date.now() });
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
        missedEvents = true;
        if (cancelled) return;
        retryTimer = setTimeout(connect, retryDelay);
        retryDelay = Math.min(retryDelay * 2, RETRY_MAX_MS);
      };
    };

    // Back in the foreground: a stream that died in the background may sit
    // in its backoff for up to a minute, so it's reopened at once (its
    // onopen then catches up). One that survived may still have missed
    // events while the phone slept, so catch up anyway.
    const onVisibilityChange = () => {
      if (document.visibilityState !== "visible") return;
      if (!eventSource || eventSource.readyState === EventSource.CLOSED) {
        if (connecting) return;
        if (retryTimer) clearTimeout(retryTimer);
        retryTimer = null;
        retryDelay = RETRY_MIN_MS;
        void connect();
      } else {
        resync();
      }
    };
    document.addEventListener("visibilitychange", onVisibilityChange);

    void connect();

    return () => {
      cancelled = true;
      document.removeEventListener("visibilitychange", onVisibilityChange);
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
    resyncedAt,
  };

  return createElement(NotificationsContext.Provider, { value }, children);
}

export function useNotifications() {
  const ctx = useContext(NotificationsContext);
  if (!ctx) throw new Error("useNotifications must be used inside <NotificationsProvider>");
  return ctx;
}
