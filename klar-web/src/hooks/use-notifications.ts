import { useEffect, useState, useCallback } from 'react';
import { useAuth } from "@/lib/auth-context";
import { notifications as notificationsApi, auth, tokens, type AppNotification } from "@/lib/api";
import { ENV } from '@/env';

const API_URL = ENV.API_URL;

export type { AppNotification };

export function useNotifications() {
  const { user } = useAuth(); // <-- 2. User-State holen
  const [notifications, setNotifications] = useState<AppNotification[]>([]);
  const [unreadCount, setUnreadCount] = useState(0);

  // Initial fetch — goes through the shared `request()` helper in lib/api.ts
  // so an expired access-token cookie gets silently refreshed and retried,
  // same as every other authenticated call. A raw fetch() here would just
  // throw on a 401 with no retry.
  useEffect(() => {
    // 3. WICHTIG: Wenn noch kein User geladen ist, gar nicht erst fetchen!
    if (!user) return;

    notificationsApi.list()
      .then((data) => {
        setNotifications(data);
        setUnreadCount(data.filter(n => !n.is_read).length);
      })
      .catch(err => console.error("Notification fetch failed:", err));
  }, [user]); // <-- 4. Hook neu ausführen, sobald der User (nach dem Refresh) da ist

  // SSE Stream
  useEffect(() => {
    // Auch der Stream darf erst starten, wenn der User da ist
    if (!user) return;

    let cancelled = false;
    let eventSource: EventSource | null = null;
    let retryTimer: ReturnType<typeof setTimeout> | null = null;

    // The access token in the URL is only ~15 minutes old at best. Once it
    // expires, this connection starts failing and — unlike a normal
    // fetch() — there's no way to update EventSource's URL/headers after
    // the fact. Without this, a tab left open past 15 minutes would have
    // its notification stream silently die until a full page reload.
    // So: on any error, refresh the token pair and reconnect with a fresh
    // one instead of relying on EventSource's own built-in reconnect
    // (which would just keep retrying with the same, now-stale token).
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
          const newNotification: AppNotification = JSON.parse(event.data);
          setNotifications(prev => [newNotification, ...prev]);
          setUnreadCount(prev => prev + 1);
        } catch (err) {
          console.error("Failed to parse SSE message", err);
        }
      };

      eventSource.onerror = () => {
        eventSource?.close();
        if (cancelled) return;

        // Try to get a fresh access token, then reconnect. If the refresh
        // token itself is invalid (genuinely logged out), give up quietly
        // rather than retrying forever.
        auth.refresh(tokens.getRefresh())
          .then((res) => {
            tokens.set(res.access_token, res.refresh_token);
            if (!cancelled) {
              retryTimer = setTimeout(connect, 1000);
            }
          })
          .catch(() => {
            // Refresh failed — session's actually gone, not just a
            // momentary network blip. Don't retry; a real login will
            // remount this hook via the `user` dependency anyway.
          });
      };
    };

    connect();

    return () => {
      cancelled = true;
      if (retryTimer) clearTimeout(retryTimer);
      eventSource?.close();
    };
  }, [user]); // <-- Abhängigkeit hier ebenfalls einfügen

  const markAllAsRead = useCallback(async () => {
    if (unreadCount === 0 || !user) return;

    setUnreadCount(0);
    setNotifications(prev => prev.map(n => ({ ...n, is_read: true })));

    notificationsApi.markRead().catch(err =>
      console.error("Failed to mark notifications as read", err)
    );
  }, [unreadCount, user]);

  return { notifications, unreadCount, markAllAsRead };
}
