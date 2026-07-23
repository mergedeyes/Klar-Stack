import { ENV } from '@/env';
const API_URL = ENV.API_URL

// ── Types ─────────────────────────────────────────────────────────────────────

export interface User {
  id: string;
  username: string;
  email: string;
  display_name: string | null;
  bio: string | null;
  avatar_url: string | null;
  email_verified: boolean;
  created_at: string;
  username_changed_at?: string | null;
}

export interface Post {
  id: string;
  user_id: string;
  username: string;
  avatar_url: string | null;
  caption: string | null;
  created_at: string;
  edited_at: string | null;
  comment_count?: number;
  thumb_url?: string | null;
  medium_url?: string | null;
  full_url?: string | null;
}

export interface MediaAsset {
  id: string;
  post_id: string;
  thumb_url: string;
  medium_url: string;
  full_url: string;
  width: number;
  height: number;
}

export interface ProfileStats {
  followers: number;
  following: number;
  posts: number;
}

export interface LikeResponse {
  liked: boolean;
  like_count: number;
}

export interface AppNotification {
  id: string;
  // 'message' only ever arrives over the SSE stream (see use-notifications.ts)
  // -- it's never persisted in the notifications table or returned by
  // notifications.list(), so the hook special-cases it instead of adding
  // it to the notification dropdown list.
  type_name: 'follow' | 'post_like' | 'comment' | 'comment_like' | 'message';
  is_read: boolean;
  created_at: string;
  post_id: string | null;
  // Raw storage key (not a full URL) for the related post's first image —
  // run through getMediaUrl() before rendering, same as Post.thumb_url.
  // Always null for 'follow' (no post involved; use actor.avatar_url
  // instead) and 'message' (also no post).
  post_thumb_url: string | null;
  actor: {
    id: string;
    username: string;
    avatar_url: string | null;
  };
}

export interface DiscoveryCursor {
  time: string;
  id: string;
}

export interface DiscoveryFeedResponse {
  data: Post[];
  next_cursor: DiscoveryCursor | null;
}

export interface Comment {
  id: string;
  post_id: string;
  user_id: string;
  username: string;
  parent_comment_id: string | null;
  body: string;
  created_at: string;
  edited_at: string | null;
  like_count: number;
  liked: boolean;
  avatar_url: string | null;
}

export interface AuthResponse {
  access_token: string;
  refresh_token: string;
  user: User;
}

export interface ApiError {
  error: string;
}

// ── Token storage ─────────────────────────────────────────────────────────────
// klarsocial.eu and klarsocial.de are genuinely different top-level domains
// sharing one backend (api.klarsocial.eu) — every request is cross-site.
// Browsers increasingly block third-party cookies outright regardless of
// SameSite/Secure config (privacy-hardened Chromium forks, Safari ITP,
// Firefox ETP), so cookies can't be relied on as the sole auth mechanism.
// Tokens are stored in localStorage and sent explicitly via
// Authorization: Bearer instead — this bypasses cookie policy entirely,
// at the accepted tradeoff of XSS-exposed storage vs. httpOnly cookies.

const ACCESS_KEY = "klar_access_token";
const REFRESH_KEY = "klar_refresh_token";

function safeGetItem(key: string): string | null {
  if (typeof window === "undefined") return null;
  return window.localStorage.getItem(key);
}

export const tokens = {
  getAccess: () => safeGetItem(ACCESS_KEY),
  getRefresh: () => safeGetItem(REFRESH_KEY),
  set: (access: string, refresh: string) => {
    if (typeof window === "undefined") return;
    window.localStorage.setItem(ACCESS_KEY, access);
    window.localStorage.setItem(REFRESH_KEY, refresh);
  },
  clear: () => {
    if (typeof window === "undefined") return;
    window.localStorage.removeItem(ACCESS_KEY);
    window.localStorage.removeItem(REFRESH_KEY);
  },
};

// ── Core fetch wrapper ────────────────────────────────────────────────────────

let isRefreshing = false;
let failedQueue: Array<{ resolve: () => void; reject: (err: any) => void }> = [];

function processQueue(error: Error | null) {
  failedQueue.forEach((prom) => {
    if (error) prom.reject(error);
    else prom.resolve();
  });
  failedQueue = [];
}

function buildFetchOptions(options: RequestInit): RequestInit {
  const headers: Record<string, string> = {
    "Content-Type": "application/json",
    ...(options.headers as Record<string, string>),
  };

  const accessToken = tokens.getAccess();
  if (accessToken) {
    headers["Authorization"] = `Bearer ${accessToken}`;
  }

  return {
    ...options,
    headers,
    // Kept for same-site/local-dev cases where the cookie does work — costs
    // nothing to also send it, and the Authorization header above is what
    // actually carries auth across the cross-site production domains.
    credentials: "include",
  };
}

async function request<T>(
  path: string,
  options: RequestInit = {},
  authenticated = false,
  _isRetry = false
): Promise<T> {
  const fetchOptions = buildFetchOptions(options);

  const res = await fetch(`${API_URL}${path}`, fetchOptions);

  if (res.status === 401 && authenticated && !_isRetry) {
    if (isRefreshing) {
      return new Promise<T>((resolve, reject) => {
        failedQueue.push({
          resolve: () => {
            resolve(
              fetch(`${API_URL}${path}`, buildFetchOptions(options)).then(async (r) => {
                if (r.status === 204) return undefined as T;
                const text = await r.text();
                return (text ? JSON.parse(text) : undefined) as T;
              })
            );
          },
          reject: (err) => reject(err),
        });
      });
    }

    isRefreshing = true;

    try {
      // Cookie is sent as a best-effort fallback (credentials: include),
      // but the refresh_token in the body is what actually carries this
      // cross-site, since third-party cookies may be blocked entirely.
      const refreshRes = await fetch(`${API_URL}/auth/refresh`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        credentials: "include",
        body: JSON.stringify({ refresh_token: tokens.getRefresh() ?? undefined }),
      });

      if (!refreshRes.ok) throw new Error("Refresh failed");

      const refreshed = (await refreshRes.json()) as { access_token: string; refresh_token: string };
      tokens.set(refreshed.access_token, refreshed.refresh_token);

      processQueue(null);

      // Retry the original request with the freshly stored access token
      const retryRes = await fetch(`${API_URL}${path}`, buildFetchOptions(options));
      if (retryRes.status === 204) return undefined as T;
      const retryText = await retryRes.text();
      return (retryText ? JSON.parse(retryText) : undefined) as T;

    } catch (error) {
      processQueue(error as Error);
      tokens.clear();
      throw new Error("Session expired. Please log in again.");
    } finally {
      isRefreshing = false;
    }
  }

  if (res.status === 204) return undefined as T;

  // Some endpoints (e.g. chat edit/reaction) return 200 with an empty body —
  // read as text first so JSON.parse isn't called on an empty string.
  const text = await res.text();
  const data = text ? JSON.parse(text) : undefined;

  if (!res.ok) {
    throw new Error((data as ApiError)?.error ?? "Something went wrong");
  }

  return data as T;
}

// ── Auth endpoints ────────────────────────────────────────────────────────────

export const auth = {
  register: (username: string, email: string, password: string) =>
    request<AuthResponse>("/auth/register", {
      method: "POST",
      body: JSON.stringify({ username, email, password }),
    }),

  login: (email: string, password: string) =>
    request<AuthResponse>("/auth/login", {
      method: "POST",
      body: JSON.stringify({ email, password }),
    }),

  logout: (refreshToken?: string | null) =>
    request<void>("/auth/logout", {
      method: "POST",
      body: JSON.stringify({ refresh_token: refreshToken ?? tokens.getRefresh() ?? undefined }),
    }),

  refresh: (refreshToken?: string | null) =>
    request<{ access_token: string; refresh_token: string }>("/auth/refresh", {
      method: "POST",
      body: JSON.stringify({ refresh_token: refreshToken ?? tokens.getRefresh() ?? undefined }),
    }),

  forgotPassword: (email: string) =>
    request<void>("/auth/forgot-password", {
      method: "POST",
      body: JSON.stringify({ email }),
    }),

  resetPassword: (token: string, password: string) =>
    request<void>("/auth/reset-password", {
      method: "POST",
      body: JSON.stringify({ token, new_password: password }),
    }),

  verifyEmail: (token: string) =>
    request<void>(`/auth/verify?token=${encodeURIComponent(token)}`),

  resendVerification: (email: string) =>
    request<void>("/auth/resend-verification", {
      method: "POST",
      body: JSON.stringify({ email }),
    }),
};

// ── User endpoints ────────────────────────────────────────────────────────────

export const users = {
  me: () => request<User>("/users/me", {}, true),
  get: (username: string) => request<User>(`/users/${username}`),
  search: (q: string, limit = 20, offset = 0) =>
    request<User[]>(
      `/users/search?q=${encodeURIComponent(q)}&limit=${limit}&offset=${offset}`
    ),
  stats: (username: string) =>
    request<ProfileStats>(`/users/${username}/stats`),

  updateProfile: (username: string | null, displayName: string | null, bio: string | null) =>
    request<User>(
      "/users/me",
      { method: "PATCH", body: JSON.stringify({ username, display_name: displayName, bio }) },
      true
    ),

  changePassword: (currentPassword: string, newPassword: string) =>
    request<void>("/users/me/password", {
      method: "PATCH",
      body: JSON.stringify({ current_password: currentPassword, new_password: newPassword }),
    }, true),

  deleteAccount: () =>
    request<void>("/users/me", { method: "DELETE" }, true),

  uploadAvatar: async (file: File): Promise<User> => {
    const form = new FormData();
    form.append("avatar", file);
    const headers: Record<string, string> = {};
    const accessToken = tokens.getAccess();
    if (accessToken) headers["Authorization"] = `Bearer ${accessToken}`;
    const res = await fetch(`${API_URL}/users/me/avatar`, {
      method: "POST",
      credentials: "include",
      headers,
      body: form,
    });
    const data = await res.json();
    if (!res.ok) throw new Error((data as ApiError).error ?? "Upload failed");
    return data as User;
  },

  // Right of access / data portability (Art. 15 + 20 DSGVO): fetches the
  // full JSON export and triggers a browser download directly — bypasses
  // the shared `request()` wrapper since we need the raw Blob and the
  // filename from Content-Disposition, not parsed JSON to use in state.
  exportData: async (): Promise<void> => {
    const headers: Record<string, string> = {};
    const accessToken = tokens.getAccess();
    if (accessToken) headers["Authorization"] = `Bearer ${accessToken}`;

    const res = await fetch(`${API_URL}/users/me/export`, {
      method: "GET",
      credentials: "include",
      headers,
    });

    if (!res.ok) {
      const data = await res.json().catch(() => null);
      throw new Error((data as ApiError)?.error ?? "Export failed");
    }

    const disposition = res.headers.get("Content-Disposition");
    const match = disposition?.match(/filename="(.+)"/);
    const filename = match?.[1] ?? "klar-datenexport.json";

    const blob = await res.blob();
    const url = window.URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = filename;
    document.body.appendChild(a);
    a.click();
    a.remove();
    window.URL.revokeObjectURL(url);
  },
};

// ── Follow endpoints ──────────────────────────────────────────────────────────

export const follows = {
  follow: (username: string) =>
    request<{ message: string }>(`/users/${username}/follow`, { method: "POST" }, true),
  unfollow: (username: string) =>
    request<{ message: string }>(`/users/${username}/follow`, { method: "DELETE" }, true),
  followers: (username: string) =>
    request<User[]>(`/users/${username}/followers`),
  following: (username: string) =>
    request<User[]>(`/users/${username}/following`),
};

// ── Post endpoints ────────────────────────────────────────────────────────────

export const posts = {
  feed: (cursor?: string, limit = 20) => {
    const params = new URLSearchParams({ limit: String(limit) });
    if (cursor) params.set("cursor", cursor);
    return request<Post[]>(`/feed?${params}`, {}, true);
  },

  discoveryFeed: (cursor?: DiscoveryCursor, limit = 15) => {
    const params = new URLSearchParams({ limit: String(limit) });
    if (cursor) {
      params.set("cursor_time", cursor.time);
      params.set("cursor_id", cursor.id);
    }
    return request<DiscoveryFeedResponse>(`/feed/discovery?${params}`, {}, true);
  },

  get: (id: string) => request<Post>(`/posts/${id}`),

  userPosts: (username: string, cursor?: string, limit = 20) => {
    const params = new URLSearchParams({ limit: String(limit) });
    if (cursor) params.set("cursor", cursor);
    return request<Post[]>(`/users/${username}/posts?${params}`);
  },

  media: (postId: string) =>
    request<MediaAsset[]>(`/posts/${postId}/media`),

  toggleLike: (postId: string) =>
    request<LikeResponse>(`/posts/${postId}/like`, { method: "POST" }, true),

  getLikes: (postId: string) =>
    request<LikeResponse>(`/posts/${postId}/likes`, {}, true),

  delete: (postId: string) =>
    request<void>(`/posts/${postId}`, { method: "DELETE" }, true),
};

// ── Block endpoints ──────────────────────────────────────────────────────────

export const notifications = {
  list: () => request<AppNotification[]>("/notifications", {}, true),
  markRead: () => request<{ message: string }>("/notifications/read", { method: "PATCH" }, true),
};

export const blocks = {
  block: (username: string) =>
    request<{ message: string }>(`/users/${username}/block`, { method: "POST" }, true),
  unblock: (username: string) =>
    request<{ message: string }>(`/users/${username}/block`, { method: "DELETE" }, true),
};

// ── Comment endpoints ─────────────────────────────────────────────────────────

export const comments = {
  list: (postId: string) =>
    request<Comment[]>(`/posts/${postId}/comments`, {}, true),

  create: (postId: string, body: string, parentCommentId?: string) =>
    request<Comment>(
      `/posts/${postId}/comments`,
      {
        method: "POST",
        body: JSON.stringify({
          body,
          parent_comment_id: parentCommentId ?? null,
        }),
      },
      true
    ),

  edit: (postId: string, commentId: string, body: string) =>
    request<Comment>(
      `/posts/${postId}/comments/${commentId}`,
      { method: "PATCH", body: JSON.stringify({ body }) },
      true
    ),

  delete: (postId: string, commentId: string) =>
    request<void>(
      `/posts/${postId}/comments/${commentId}`,
      { method: "DELETE" },
      true
    ),

  toggleLike: (postId: string, commentId: string) =>
    request<LikeResponse>(
      `/posts/${postId}/comments/${commentId}/like`,
      { method: "POST" },
      true
    ),
};

// --- CHAT API ---

export interface ReactionEntry {
  emoji: string;
  user_id: string;
  username: string;
}

export interface Conversation {
  id: string;
  other_user_id: string;
  other_username: string;
  other_avatar_url: string | null;
  last_message: string | null;
  updated_at: string;
}

export interface ChatMessage {
  id: string;
  conversation_id: string;
  sender_id: string;
  body: string;
  created_at: string;
  edited_at: string | null;
  is_read: boolean;
  reply_to_message_id: string | null;
  reactions: ReactionEntry[];
}

export const chatsApi = {
  getConversations: () =>
    request<Conversation[]>("/chats", {}, true),

  getMessages: (conversationId: string) =>
    request<ChatMessage[]>(`/chats/${conversationId}/messages`, {}, true),

  sendMessage: (receiverId: string, body: string, replyToId?: string) =>
    request<ChatMessage>(
      "/chats/send",
      {
        method: "POST",
        body: JSON.stringify({ receiver_id: receiverId, body, reply_to_message_id: replyToId || null }),
      },
      true
    ),

  editMessage: (messageId: string, body: string) =>
    request<void>(
      `/chats/messages/${messageId}`,
      { method: "PATCH", body: JSON.stringify({ body }) },
      true
    ),

  deleteMessage: (messageId: string) =>
    request<void>(`/chats/messages/${messageId}`, { method: "DELETE" }, true),

  toggleReaction: (messageId: string, emoji: string) =>
    request<void>(
      `/chats/messages/${messageId}/reactions`,
      { method: "POST", body: JSON.stringify({ emoji }) },
      true
    ),

  // Total unread messages across every conversation, for the Chat icon's
  // red-dot badge (see use-notifications.ts, which also updates this
  // count live via the 'message' SSE event without re-fetching).
  getUnreadCount: () =>
    request<{ count: number }>("/chats/unread-count", {}, true),

  // Called when a conversation is opened, so its messages stop counting
  // toward the unread badge.
  markConversationRead: (conversationId: string) =>
    request<void>(`/chats/${conversationId}/read`, { method: "PATCH" }, true),
};
