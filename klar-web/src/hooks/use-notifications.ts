import { useEffect, useState, useCallback } from 'react';
import { useAuth } from "@/lib/auth-context";
import { notifications as notificationsApi, type AppNotification } from "@/lib/api";
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

    const eventSource = new EventSource(`${API_URL}/notifications/stream`, {
      withCredentials: true
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

    return () => eventSource.close();
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
