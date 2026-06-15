import { useEffect, useState, useCallback } from 'react';
import { useAuth } from "@/lib/auth-context"; // <-- 1. Auth importieren

const API_URL = process.env.NEXT_PUBLIC_API_URL ?? "http://localhost:3000";

export interface AppNotification {
  id: string;
  type_name: 'follow' | 'post_like' | 'comment' | 'comment_like';
  is_read: boolean;
  created_at: string;
  post_id: string | null;
  actor: {
    id: string;
    username: string;
    avatar_url: string | null;
  };
}

export function useNotifications() {
  const { user } = useAuth(); // <-- 2. User-State holen
  const [notifications, setNotifications] = useState<AppNotification[]>([]);
  const [unreadCount, setUnreadCount] = useState(0);

  // Initial fetch
  useEffect(() => {
    // 3. WICHTIG: Wenn noch kein User geladen ist, gar nicht erst fetchen!
    if (!user) return; 

    fetch(`${API_URL}/notifications`, {
      credentials: "include",
    })
      .then(async (res) => {
        if (!res.ok) throw new Error(`HTTP ${res.status}: ${res.statusText}`);
        const text = await res.text();
        return text ? JSON.parse(text) : [];
      })
      .then((data: AppNotification[]) => {
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

    fetch(`${API_URL}/notifications/read`, {
      method: 'PATCH',
      credentials: "include" 
    })
    .then(res => {
      if (!res.ok) console.error("Failed to mark notifications as read", res.status);
    })
    .catch(console.error);
  }, [unreadCount, user]);

  return { notifications, unreadCount, markAllAsRead };
}