import { ENV } from '../env';
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

// ── Token storage (Stubs so existing files don't break) ───────────────

export const tokens = {
  getAccess: () => "cookie-auth-active",
  getRefresh: () => "cookie-auth-active",
  set: (_access: string, _refresh: string) => {},
  clear: () => {},
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

async function request<T>(
  path: string,
  options: RequestInit = {},
  authenticated = false,
  _isRetry = false
): Promise<T> {
  const headers: Record<string, string> = {
    "Content-Type": "application/json",
    ...(options.headers as Record<string, string>),
  };

  // ALL requests must include credentials so cookies are sent automatically
  const fetchOptions: RequestInit = {
    ...options,
    headers,
    credentials: "include", 
  };

  const res = await fetch(`${API_URL}${path}`, fetchOptions);

  if (res.status === 401 && authenticated && !_isRetry) {
    if (isRefreshing) {
      return new Promise<T>((resolve, reject) => {
        failedQueue.push({
          resolve: () => {
            resolve(fetch(`${API_URL}${path}`, fetchOptions).then(r => r.status === 204 ? undefined as T : r.json()));
          },
          reject: (err) => reject(err),
        });
      });
    }

    isRefreshing = true;

    try {
      // The browser will automatically attach the httpOnly refresh_token cookie
      const refreshRes = await fetch(`${API_URL}/auth/refresh`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        credentials: "include",
      });

      if (!refreshRes.ok) throw new Error("Refresh failed");
      
      processQueue(null);

      // Retry the original request
      const retryRes = await fetch(`${API_URL}${path}`, fetchOptions);
      if (retryRes.status === 204) return undefined as T;
      return await retryRes.json() as T;

    } catch (error) {
      processQueue(error as Error);
      throw new Error("Session expired. Please log in again.");
    } finally {
      isRefreshing = false;
    }
  }

  if (res.status === 204) return undefined as T;

  const data = await res.json();

  if (!res.ok) {
    throw new Error((data as ApiError).error ?? "Something went wrong");
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

  logout: (_ignored?: string) =>
    request<void>("/auth/logout", {
      method: "POST",
    }),

  refresh: (_ignored?: string) =>
    request<{ access_token: string; refresh_token: string }>("/auth/refresh", {
      method: "POST",
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
    const res = await fetch(`${API_URL}/users/me/avatar`, {
      method: "POST",
      credentials: "include", 
      body: form,
    });
    const data = await res.json();
    if (!res.ok) throw new Error((data as ApiError).error ?? "Upload failed");
    return data as User;
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
  getConversations: async (): Promise<Conversation[]> => {
    const res = await fetch(`${API_URL}/chats`, { credentials: "include" });
    if (!res.ok) throw new Error("Failed to fetch conversations");
    return res.json();
  },
  
  getMessages: async (conversationId: string): Promise<ChatMessage[]> => {
    const res = await fetch(`${API_URL}/chats/${conversationId}/messages`, { credentials: "include" });
    if (!res.ok) throw new Error("Failed to fetch messages");
    return res.json();
  },

  sendMessage: async (receiverId: string, body: string, replyToId?: string): Promise<ChatMessage> => {
    const res = await fetch(`${API_URL}/chats/send`, {
      method: "POST",
      credentials: "include",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ receiver_id: receiverId, body, reply_to_message_id: replyToId || null }),
    });
    if (!res.ok) {
        const errData = await res.json().catch(() => ({}));
        throw new Error(errData.error || "Failed to send message");
    }
    return res.json();
  },

  editMessage: async (messageId: string, body: string): Promise<void> => {
    const res = await fetch(`${API_URL}/chats/messages/${messageId}`, {
      method: "PATCH",
      credentials: "include",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ body }),
    });
    if (!res.ok) throw new Error("Failed to edit message");
  },

  deleteMessage: async (messageId: string): Promise<void> => {
    const res = await fetch(`${API_URL}/chats/messages/${messageId}`, {
      method: "DELETE",
      credentials: "include",
    });
    if (!res.ok) throw new Error("Failed to delete message");
  },

  toggleReaction: async (messageId: string, emoji: string): Promise<void> => {
    const res = await fetch(`${API_URL}/chats/messages/${messageId}/reactions`, {
      method: "POST",
      credentials: "include",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ emoji }),
    });
    if (!res.ok) throw new Error("Failed to toggle reaction");
  }
};