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
  is_private: boolean;
  // The *caller's* relationship to this profile. Populated by
  // GET /users/:username, GET /users/me and (when logged in) user search,
  // whose cards have a follow button -- other endpoints that also return a
  // User-shaped object (followers/following lists) omit it.
  viewer_relationship?: 'self' | 'following' | 'requested' | 'not_following' | null;
  // Reverse direction: does *this* profile have a pending request to
  // follow *me*? Lets accept/decline show up right on their profile page,
  // not just in the notification dropdown. Always false for your own
  // profile or when logged out.
  incoming_follow_request?: boolean;
  // Whether this account is an admin/moderator (server computes this
  // against ADMIN_EMAILS -- see reports.rs / utils.rs). Only ever
  // populated by GET /users/me; login/register/refresh responses don't
  // set it, so it's undefined (falsy) until the next /users/me fetch --
  // matches viewer_relationship/incoming_follow_request's optionality
  // above. Display-only: the backend enforces the real check on every
  // /admin/* route regardless of what this says.
  is_admin?: boolean;
  // Whether this account consented to its likes and comments being
  // logged to rank the Discovery page (the home feed never is). Only
  // GET /users/me sets it, for the owner.
  personalization_enabled?: boolean;
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
  // "hidden" posts never reach the client at all (server-side filtered)
  // except for the owner viewing their own profile -- "flagged" ones do
  // reach the client, and should render behind an interstitial warning.
  // "removed" ones (by the moderation team) never reach anyone.
  moderation_status?: 'visible' | 'flagged' | 'hidden' | 'removed';
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
  // 'message_changed' (an edited or deleted message) likewise only
  // arrives live, to refresh an open chat.
  type_name: 'follow' | 'post_like' | 'comment' | 'comment_like' | 'message' | 'message_changed' | 'follow_request' | 'follow_accepted'
    // Notices from Klar itself (no actor): a statement of reasons about
    // your content, the outcome of a report you filed, or the answer to
    // your objection.
    | 'moderation_decision' | 'report_outcome' | 'objection_resolved';
  is_read: boolean;
  created_at: string;
  post_id: string | null;
  // Raw storage key (not a full URL) for the related post's first image —
  // run through getMediaUrl() before rendering, same as Post.thumb_url.
  // Always null for 'follow'/'follow_request'/'follow_accepted' (no post
  // involved; use actor.avatar_url instead) and 'message' (also no post).
  post_thumb_url: string | null;
  // null for notices from Klar itself (the moderation types above).
  actor: {
    id: string;
    username: string;
    avatar_url: string | null;
  } | null;
  // The statement of reasons a moderation notice refers to.
  decision_id: string | null;
}

/** Keyset cursor for post lists: the last post's created_at plus its id,
 *  which breaks ties between posts with the same timestamp. */
export interface PostCursor {
  time: string;
  id: string;
}

/** Cursor for the page after a non-empty `page` of posts. */
export function cursorAfter(page: Post[]): PostCursor {
  const last = page[page.length - 1];
  return { time: last.created_at, id: last.id };
}

export interface DiscoveryFeedResponse {
  data: Post[];
  next_cursor: PostCursor | null;
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
  // "hidden" comments never reach the client except for their own author
  // (server-side filtered) -- "flagged" ones do reach the client and
  // should render behind a lightweight interstitial for non-authors.
  // "removed": removed by the moderation team; only sent, with an empty
  // body, when others replied to it, as a placeholder for the thread.
  moderation_status?: 'visible' | 'flagged' | 'hidden' | 'removed';
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

// ── Session refresh ───────────────────────────────────────────────────────────
// Refresh tokens are single-use (the backend deletes one as it redeems it),
// so every refresh in this tab has to go through refreshSession(), which
// shares one in-flight request. Two independent refreshes with the same
// token would race, and the loser would log the user out.

/** The server rejected the refresh token: the session is really over. */
export class SessionExpiredError extends Error {
  constructor() {
    super("Session expired. Please log in again.");
    this.name = "SessionExpiredError";
  }
}

let refreshInFlight: Promise<void> | null = null;

export function refreshSession(): Promise<void> {
  if (!refreshInFlight) {
    refreshInFlight = doRefresh().finally(() => {
      refreshInFlight = null;
    });
  }
  return refreshInFlight;
}

async function doRefresh(): Promise<void> {
  const sent = tokens.getRefresh();

  // Cookie is sent as a best-effort fallback (credentials: include), but
  // the refresh_token in the body is what actually carries this cross-site,
  // since third-party cookies may be blocked entirely.
  const res = await fetch(`${API_URL}/auth/refresh`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    credentials: "include",
    body: JSON.stringify({ refresh_token: sent ?? undefined }),
  });

  if (res.ok) {
    const refreshed = (await res.json()) as { access_token: string; refresh_token: string };
    tokens.set(refreshed.access_token, refreshed.refresh_token);
    return;
  }

  // Another tab shares this localStorage and may have redeemed the same
  // token a moment earlier. If storage now holds a different refresh token,
  // that tab's refresh succeeded and its new tokens are ours to use too.
  const current = tokens.getRefresh();
  if (current && current !== sent) return;

  throw new SessionExpiredError();
}

// Refresh this long before the access token actually expires, so a request
// never leaves with a token that dies in flight.
const EXPIRY_MARGIN_MS = 30_000;

function accessTokenExpiresAt(token: string): number | null {
  try {
    const payload = token.split(".")[1].replace(/-/g, "+").replace(/_/g, "/");
    const { exp } = JSON.parse(atob(payload));
    return typeof exp === "number" ? exp * 1000 : null;
  } catch {
    return null;
  }
}

/**
 * The stored access token, refreshed first if it's about to expire.
 * Refreshing ahead of time (instead of only after a 401) matters for the
 * endpoints with optional auth -- profiles, posts, media: an expired token
 * there doesn't produce a 401, it silently makes you anonymous, so a
 * private account you follow would show as "This account is private".
 */
export async function getAccessToken(): Promise<string | null> {
  const token = tokens.getAccess();
  if (!token || !tokens.getRefresh()) return token;

  const expiresAt = accessTokenExpiresAt(token);
  if (expiresAt !== null && expiresAt - Date.now() < EXPIRY_MARGIN_MS) {
    try {
      await refreshSession();
    } catch (err) {
      if (err instanceof SessionExpiredError) tokens.clear();
      // On a network error, carry on with the old token; the request
      // itself will fail or 401 and be handled there.
    }
  }
  return tokens.getAccess();
}

// ── Core fetch wrapper ────────────────────────────────────────────────────────

async function buildFetchOptions(options: RequestInit): Promise<RequestInit> {
  const headers: Record<string, string> = {
    ...(options.headers as Record<string, string>),
  };
  // FormData needs the browser to set its own multipart Content-Type
  // (with the boundary), so only default to JSON for everything else.
  if (!(options.body instanceof FormData) && !headers["Content-Type"]) {
    headers["Content-Type"] = "application/json";
  }

  const accessToken = await getAccessToken();
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

/**
 * fetch() against the API with auth attached. For `authenticated` calls,
 * a 401 triggers one refresh and one retry. Returns the raw Response, for
 * callers that need more than parsed JSON (e.g. the data export's Blob).
 */
export async function apiFetch(
  path: string,
  options: RequestInit = {},
  authenticated = false
): Promise<Response> {
  const url = `${API_URL}${path}`;
  const res = await fetch(url, await buildFetchOptions(options));

  if (res.status !== 401 || !authenticated || !tokens.getRefresh()) return res;

  try {
    await refreshSession();
  } catch (err) {
    if (err instanceof SessionExpiredError) tokens.clear();
    throw err;
  }
  return fetch(url, await buildFetchOptions(options));
}

// Reads a response body as JSON if it is JSON. Some endpoints return an
// empty body, and axum's own rejections (malformed JSON, body too large)
// are plain text, so neither may go through a bare JSON.parse.
async function parseBody(res: Response): Promise<unknown> {
  const text = await res.text();
  if (!text) return undefined;
  try {
    return JSON.parse(text);
  } catch {
    return text;
  }
}

// An API error with its HTTP status, for the few places that react to a
// specific one (e.g. 423 Locked on login). Still an Error, so every other
// caller keeps using err.message.
export class HttpError extends Error {
  constructor(message: string, public status: number) {
    super(message);
    this.name = "HttpError";
  }
}

function errorMessage(data: unknown, status: number): string {
  if (data && typeof data === "object" && typeof (data as ApiError).error === "string") {
    return (data as ApiError).error;
  }
  if (typeof data === "string" && data.length <= 200) return data;
  return `Something went wrong (${status})`;
}

async function request<T>(
  path: string,
  options: RequestInit = {},
  authenticated = false
): Promise<T> {
  const res = await apiFetch(path, options, authenticated);
  if (res.status === 204) return undefined as T;

  const data = await parseBody(res);
  if (!res.ok) throw new HttpError(errorMessage(data, res.status), res.status);

  return data as T;
}

// ── Auth endpoints ────────────────────────────────────────────────────────────

export const auth = {
  // acceptTerms is required and enforced server-side (POST /auth/register
  // rejects the request with 400 if it isn't true) -- see the register
  // handler and models::RegisterRequest on the backend. Previously this
  // checkbox was frontend-only and never reached the API at all.
  register: (username: string, email: string, password: string, acceptTerms: boolean) =>
    request<AuthResponse>("/auth/register", {
      method: "POST",
      body: JSON.stringify({ username, email, password, accept_terms: acceptTerms }),
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

  // After a 423 on login: the account was locked because a takeover is
  // suspected; this sends the owner a new reset link (rate-limited).
  resendLockLink: (email: string, password: string) =>
    request<{ message: string }>("/auth/locked/resend-link", {
      method: "POST",
      body: JSON.stringify({ email, password }),
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
  // Test phase only: keep this account through the pre-launch wipe.
  keepAccount: () => request<{ keep: boolean; since: string | null }>("/users/me/keep-account", {}, true),
  setKeepAccount: (keep: boolean) =>
    request<{ keep: boolean; since: string | null }>(
      "/users/me/keep-account",
      { method: "PATCH", body: JSON.stringify({ keep }) },
      true
    ),
  me: () => request<User>("/users/me", {}, true),
  // Gives or withdraws the consent; withdrawing deletes the log (handlers/events.rs).
  setPersonalization: (enabled: boolean) =>
    request<{ enabled: boolean }>(
      "/users/me/personalization",
      { method: "PATCH", body: JSON.stringify({ enabled }) },
      true
    ),
  get: (username: string) => request<User>(`/users/${username}`),
  search: (q: string, limit = 20, offset = 0) =>
    request<User[]>(
      `/users/search?q=${encodeURIComponent(q)}&limit=${limit}&offset=${offset}`
    ),
  stats: (username: string) =>
    request<ProfileStats>(`/users/${username}/stats`),

  updateProfile: (username: string | null, displayName: string | null, bio: string | null, isPrivate?: boolean | null) =>
    request<User>(
      "/users/me",
      { method: "PATCH", body: JSON.stringify({ username, display_name: displayName, bio, is_private: isPrivate ?? null }) },
      true
    ),

  // Ends every session, and returns fresh tokens so this device stays
  // signed in; they're stored here.
  changePassword: async (currentPassword: string, newPassword: string) => {
    const fresh = await request<{ access_token: string; refresh_token: string }>("/users/me/password", {
      method: "PATCH",
      body: JSON.stringify({ current_password: currentPassword, new_password: newPassword }),
    }, true);
    tokens.set(fresh.access_token, fresh.refresh_token);
  },

  // Asks for the password: a session alone can't delete an account.
  deleteAccount: (password: string) =>
    request<void>("/users/me", { method: "DELETE", body: JSON.stringify({ password }) }, true),

  deleteAvatar: () => request<User>("/users/me/avatar", { method: "DELETE" }, true),

  uploadAvatar: (file: File) => {
    const form = new FormData();
    form.append("avatar", file);
    return request<User>("/users/me/avatar", { method: "POST", body: form }, true);
  },

  // Right of access / data portability (Art. 15 + 20 DSGVO): fetches the
  // full JSON export and triggers a browser download directly — uses
  // apiFetch rather than `request()` since we need the raw Blob and the
  // filename from Content-Disposition, not parsed JSON to use in state.
  exportData: async (): Promise<void> => {
    await saveDownload(await apiFetch("/users/me/export", {}, true), "klar-datenexport.zip");
  },
};

/** Hands a file response to the browser as a download, named as the
 * server's Content-Disposition says. */
async function saveDownload(res: Response, fallbackName: string): Promise<void> {
  if (!res.ok) {
    throw new Error(errorMessage(await parseBody(res), res.status));
  }

  const disposition = res.headers.get("Content-Disposition");
  const match = disposition?.match(/filename="(.+)"/);
  const filename = match?.[1] ?? fallbackName;

  const blob = await res.blob();
  const url = window.URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  a.remove();
  window.URL.revokeObjectURL(url);
}

// An earlier audit export, as the log lists it.
export interface AuditExportEntry {
  id: string;
  created_at: string;
  exported_by_username: string | null;
  period_from: string;
  period_to: string;
  with_identities: boolean;
  reason: string;
}

export const adminAuditApi = {
  list: () => request<AuditExportEntry[]>("/admin/audit-exports", {}, true),
  // Logged on the server before the file comes back (handlers/audit_export.rs).
  download: async (from: string, to: string, withIdentities: boolean, reason: string): Promise<void> => {
    const res = await apiFetch(
      "/admin/audit-exports",
      {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ from, to, with_identities: withIdentities, reason }),
      },
      true,
    );
    await saveDownload(res, `klar-audit-${from}_${to}.zip`);
  },
};

// ── Follow endpoints ──────────────────────────────────────────────────────────

export interface FollowActionResponse {
  message: string;
  // "following" for an immediate/accepted follow, "requested" if it went
  // to a private account and is now pending, "not_following" after an
  // unfollow/cancel-request.
  status: 'following' | 'requested' | 'not_following';
}

export const follows = {
  follow: (username: string) =>
    request<FollowActionResponse>(`/users/${username}/follow`, { method: "POST" }, true),
  // Also cancels a pending request, if that's what actually exists —
  // "unfollow" here really means "stop following / withdraw my request".
  unfollow: (username: string) =>
    request<FollowActionResponse>(`/users/${username}/follow`, { method: "DELETE" }, true),
  followers: (username: string) =>
    request<User[]>(`/users/${username}/followers`),
  following: (username: string) =>
    request<User[]>(`/users/${username}/following`),
};

// ── Follow request endpoints (private accounts) ──────────────────────────────

export interface FollowRequest {
  requester_id: string;
  requester_username: string;
  requester_display_name: string | null;
  requester_avatar_url: string | null;
  created_at: string;
}

export const followRequestsApi = {
  list: () => request<FollowRequest[]>("/users/me/follow-requests", {}, true),
  accept: (requesterUsername: string) =>
    request<void>(`/users/me/follow-requests/${requesterUsername}/accept`, { method: "POST" }, true),
  reject: (requesterUsername: string) =>
    request<void>(`/users/me/follow-requests/${requesterUsername}/reject`, { method: "POST" }, true),
};

// ── Reporting & moderation ────────────────────────────────────────────────────

export type ReportReason =
  | 'spam' | 'harassment' | 'hate_speech' | 'violence'
  | 'self_harm' | 'sexual_content' | 'csam' | 'impersonation' | 'other'
  | 'fraud' | 'ncii' | 'terrorism' | 'illegal_goods' | 'extremism'
  // Only on decisions from a rights claim; not a report reason users pick.
  | 'copyright';

export type ReportTargetType = 'post' | 'comment' | 'user' | 'message';

// Where a report came from: someone using the app, the public notice form,
// the team itself, or an authority's order.
export type ReportSource = "user_report" | "public_notice" | "own_initiative" | "authority_order";

// One pending report, as listed under its item in the queue.
export interface QueueReport {
  id: string;
  // Both null once the reporter has deleted their account, and for a
  // notice from the public form.
  reporter_id: string | null;
  reporter_username: string | null;
  reason: ReportReason;
  details: string | null;
  created_at: string;
  source: ReportSource;
  authority: string | null;
  order_reference: string | null;
  // The reporter asked to have a dismissal checked again.
  recheck_requested_at: string | null;
  recheck_note: string | null;
  // A notice from the public form: who sent it, if they said.
  notifier_name: string | null;
  notifier_email: string | null;
}

// An objection against an automatic restriction that rests on the group's
// reports: deciding the reports answers it.
export interface QueueObjection {
  decision_id: string;
  restriction: "hidden" | "flagged";
  objection: string | null;
  objected_at: string | null;
}

// All pending reports on one item. Dismissing or removing acts on the
// whole group.
export interface ReportGroup {
  target_type: ReportTargetType;
  target_id: string;
  // Post caption or comment body; null for profiles and messages (a
  // message is only shown through its evidence record).
  target_preview: string | null;
  // Never set for a group with a CSAM or intimate-image report.
  target_thumb_url: string | null;
  target_username: string | null;
  target_exists: boolean;
  target_status: "visible" | "flagged" | "hidden" | "removed" | null;
  evidence_id: string | null;
  evidence_content_deleted: boolean | null;
  severity: "critical" | "high" | "normal";
  // A report in the group has waited for more than 30 days.
  overdue: boolean;
  reports: QueueReport[];
  objections: QueueObjection[];
}

export const reportsApi = {
  create: (targetType: ReportTargetType, targetId: string, reason: ReportReason, details?: string) =>
    request<{ id: string }>(
      "/reports",
      { method: "POST", body: JSON.stringify({ target_type: targetType, target_id: targetId, reason, details: details || null }) },
      true
    ),
};

// A closed report, as GET /admin/reports/closed lists it: what the queue
// decided, by whom and when. No content; the decision and the evidence
// record are linked instead.
export interface ClosedReport {
  id: string;
  created_at: string;
  reviewed_at: string | null;
  reviewed_by_username: string | null;
  status: "actioned" | "dismissed";
  outcome: ReportOutcome | null;
  reason: string;
  source: ReportSource;
  target_type: string;
  target_id: string;
  target_username: string | null;
  reporter_username: string | null;
  authority: string | null;
  order_reference: string | null;
  review_note: string | null;
  recheck_requested_at: string | null;
  decision_id: string | null;
  evidence_id: string | null;
}

export interface ClosedReportsFilter {
  outcome?: ReportOutcome;
  before_time?: string;
  before_id?: string;
  limit?: number;
}

export const adminReportsApi = {
  list: () => request<ReportGroup[]>("/admin/reports", {}, true),
  closed: (filter: ClosedReportsFilter) => {
    const query = new URLSearchParams();
    for (const [key, value] of Object.entries(filter)) {
      if (value !== undefined && value !== "") query.set(key, String(value));
    }
    return request<ClosedReport[]>(`/admin/reports/closed?${query}`, {}, true);
  },
  // Closes every pending report on the item, or just this one (onlyThis).
  // objectionResponse answers a pending objection against an automatic
  // restriction the dismissal lifts; it is shown to the author.
  dismiss: (reportId: string, note?: string, onlyThis = false, objectionResponse?: string) =>
    request<void>(
      `/admin/reports/${reportId}/dismiss`,
      {
        method: "POST",
        body: JSON.stringify({ note: note || null, only_this: onlyThis, objection_response: objectionResponse || null }),
      },
      true
    ),
  // violation: a catalog id (GET /admin/violations) or "none"; omitted, the
  // backend uses the report reason's first type. justification is required
  // for a type of another reason than the report's, and for "none".
  remove: (reportId: string, note?: string, violation?: string, justification?: string) =>
    request<void>(
      `/admin/reports/${reportId}/remove`,
      {
        method: "POST",
        body: JSON.stringify({ note: note || null, violation: violation ?? null, justification: justification || null }),
      },
      true
    ),
  // A case without a report: the team's own finding, or an authority's
  // order (authority required). It lands in the queue as a report.
  createCase: (c: {
    target_type: "post" | "comment" | "user";
    target_id: string;
    reason: ReportReason;
    details?: string;
    source: "own_initiative" | "authority_order";
    authority?: string;
    order_reference?: string;
  }) =>
    request<{ report_id: string }>(
      "/admin/cases",
      {
        method: "POST",
        body: JSON.stringify({
          ...c,
          details: c.details || null,
          authority: c.authority || null,
          order_reference: c.order_reference || null,
        }),
      },
      true
    ),
};

// ── Account standing (strikes, warnings, suspensions) ────────────────────────

export type StrikeSeverity = "none" | "minor" | "moderate" | "serious" | "grave" | "severe";
export type AccountMeasure = "warning" | "suspend_7d" | "suspend_30d" | "ban";

// One entry of the violation catalog (Klar/src/standing.rs): what the
// removed content was, with a written criterion and fixed points.
export interface Violation {
  id: string;
  reason: ReportReason;
  severity: StrikeSeverity;
  label: string;
  label_de: string;
  criterion_de: string;
  authority_report: "none" | "recommended" | "required";
}

export interface Strike {
  id: string;
  decision_id: string;
  violation: string;
  violation_label: string;
  violation_label_de: string;
  severity: Exclude<StrikeSeverity, "none">;
  // What the admin gave; points is higher when the repeat factor applied
  // (third strike for the same reason within 30 days counts 1.5 times).
  base_points: number;
  points: number;
  created_at: string;
  // null: doesn't expire (severe violations).
  expires_at: string | null;
  reason: ReportReason;
  target_type: ReportTargetType;
  content_excerpt: string | null;
}

export type ProfileField = "avatar" | "bio" | "display_name" | "username";

export interface Suspension {
  // null for a permanent suspension.
  until: string | null;
  permanent: boolean;
  // Permanent suspensions: when the account is deleted (null while an
  // objection is pending or the statement is held back, since deletion
  // waits for both).
  deletion_at: string | null;
  // Admins only: the measure's statement is still held back (CSAM), which
  // blocks the deletion until an admin sends it.
  statement_held?: boolean;
}

export interface MyStanding {
  score: number;
  max_score: number;
  suspension: Suspension | null;
  strikes: Strike[];
  thresholds: { score: number; measure: AccountMeasure }[];
}

export interface AccountMeasureRecord {
  id: string;
  restriction: "warning" | "suspended" | "banned";
  reason: ReportReason;
  suspension_days: number | null;
  standing_score: number | null;
  created_at: string;
  lifted_at: string | null;
  superseded: boolean;
  delivered: boolean;
  objection_status: "pending" | "rejected" | "accepted" | null;
}

export interface AdminStanding extends MyStanding {
  user_id: string;
  username: string;
  // What the score suggests next; the admin decides.
  suggestion: AccountMeasure | null;
  measures: AccountMeasureRecord[];
  // Pending reports on the account itself, which a measure can answer.
  pending_reports: PendingAccountReport[];
}

export interface PendingAccountReport {
  id: string;
  reason: string;
  source: ReportSource;
  created_at: string;
}

export const standingApi = {
  mine: () => request<MyStanding>("/users/me/standing", {}, true),
};

// What an admin sees when opening a strike: the removed content as it was,
// its context and the reports on it. Opening is logged.
export interface StrikeDetail extends Strike {
  username: string;
  snapshot: {
    content: {
      type: "post" | "comment" | "message" | "profile";
      text?: string | null;
      // A profile: what was removed.
      removed?: Record<string, unknown>;
      created_at: string;
      edited_at: string | null;
      image_count?: number;
    } | null;
    context: {
      post: { id: string; text: string | null; author: string; created_at: string } | null;
      parent_comment: { text: string; author: string; created_at: string } | null;
    } | null;
    reports: { reason: ReportReason; details: string | null; created_at: string }[];
    removed_at: string;
  };
  criterion_de: string | null;
  // Set when the admin classified it under another reason than the report's.
  reported_reason: ReportReason | null;
  justification: string | null;
  decided_by: string | null;
  decided_at: string;
  evidence_id: string | null;
}

export const adminStandingApi = {
  list: () => request<AdminStanding[]>("/admin/standing", {}, true),
  violations: () => request<Violation[]>("/admin/violations", {}, true),
  openStrike: (id: string) =>
    request<StrikeDetail>(`/admin/strikes/${id}/open`, { method: "POST", body: "{}" }, true),
  get: (username: string) =>
    request<AdminStanding>(`/admin/users/${encodeURIComponent(username)}/standing`, {}, true),
  // explanation: shown to the user; required when the account has no
  // active strike or the measure goes beyond the suggestion. reportIds:
  // pending reports on the account that the measure answers.
  apply: (username: string, measure: AccountMeasure, reason: ReportReason, explanation?: string, reportIds: string[] = []) =>
    request<AdminStanding>(
      `/admin/users/${encodeURIComponent(username)}/measures`,
      { method: "POST", body: JSON.stringify({ measure, reason, explanation: explanation || null, report_ids: reportIds }) },
      true
    ),
  // Removes parts of a profile, like a content removal: classified, with a
  // strike and a statement; an accepted objection puts them back.
  removeFromProfile: (
    username: string,
    fields: ProfileField[],
    violation?: string,
    justification?: string,
    note?: string,
    reportIds: string[] = []
  ) =>
    request<void>(
      `/admin/users/${encodeURIComponent(username)}/profile-removal`,
      {
        method: "POST",
        body: JSON.stringify({
          fields,
          violation: violation ?? null,
          justification: justification || null,
          note: note || null,
          report_ids: reportIds,
        }),
      },
      true
    ),
  lift: (username: string) =>
    request<void>(`/admin/users/${encodeURIComponent(username)}/lift-suspension`, { method: "POST", body: "{}" }, true),
};

// ── Evidence (admin) ──────────────────────────────────────────────────────────
// Preserved copies of content reported for a likely-illegal reason: the state
// when reported plus every edit since. Everything that shows content takes a
// reason, which the backend logs before answering.

export interface EvidenceSummary {
  id: string;
  target_type: ReportTargetType;
  target_id: string;
  reasons: ReportReason[];
  created_at: string;
  // Set once the original was deleted, with what deleted it.
  content_deleted_at: string | null;
  deletion_trigger: "moderation_removal" | "user_deletion" | "account_deletion" | null;
  decision: "removed" | "dismissed" | null;
  decided_at: string | null;
  retain_until: string | null;
  legal_hold: boolean;
  purged_at: string | null;
  version_count: number;
  file_count: number;
  // Undecided 30 days after it was opened.
  overdue: boolean;
  // Set when the removal's classification says it goes to the authorities:
  // "required" (threat to life or safety, DSA Art. 18; never purged before
  // a report is recorded) or "recommended". authority_reported: a report
  // was recorded on the record.
  authority_report: "required" | "recommended" | null;
  authority_reported: boolean;
}

export interface EvidenceFile {
  id: string;
  kind: "post_media" | "avatar";
  content_type: string;
  size_bytes: number | null;
  sha256: string | null;
  // null while the copy into the evidence zone is still pending.
  copied_at: string | null;
}

export interface EvidenceEvent {
  id: string;
  actor_id: string | null;
  actor_username: string | null;
  action: string;
  reason: string | null;
  details: Record<string, unknown> | null;
  created_at: string;
}

export interface EvidenceVersion {
  id: string;
  captured_at: string;
  cause: "reported" | "edited" | "deleted";
  // The item, its author and (for comments) its context; the shape
  // depends on target_type.
  content: Record<string, unknown>;
  // Files first captured in this version (post images come with the
  // first one, a new avatar with the edit that set it).
  files: EvidenceFile[];
}

export interface EvidenceReport {
  id: string;
  reason: ReportReason;
  details: string | null;
  status: "pending" | "dismissed" | "actioned";
  created_at: string;
  reporter_id: string | null;
}

export interface EvidenceDetail extends EvidenceSummary {
  decided_by: string | null;
  decision_note: string | null;
  // Oldest first; empty once purged.
  versions: EvidenceVersion[];
  reports: EvidenceReport[];
  events: EvidenceEvent[];
}

export const adminEvidenceApi = {
  list: (includePurged = false) =>
    request<EvidenceSummary[]>(`/admin/evidence?include_purged=${includePurged}`, {}, true),
  open: (id: string, reason: string) =>
    request<EvidenceDetail>(
      `/admin/evidence/${id}/open`,
      { method: "POST", body: JSON.stringify({ reason }) },
      true
    ),
  // Returns an object URL for the file; the caller revokes it. POST keeps
  // the reason out of the URL.
  fileUrl: async (id: string, fileId: string, reason: string): Promise<string> => {
    const res = await apiFetch(
      `/admin/evidence/${id}/files/${fileId}`,
      { method: "POST", body: JSON.stringify({ reason }) },
      true
    );
    if (!res.ok) throw new Error(errorMessage(await parseBody(res), res.status));
    return URL.createObjectURL(await res.blob());
  },
  setHold: (id: string, hold: boolean, reason: string) =>
    request<EvidenceSummary>(
      `/admin/evidence/${id}/hold`,
      { method: "POST", body: JSON.stringify({ hold, reason }) },
      true
    ),
  recordAuthorityReport: (id: string, authority: string, reportedOn: string, reference?: string, note?: string) =>
    request<void>(
      `/admin/evidence/${id}/authority-report`,
      {
        method: "POST",
        body: JSON.stringify({ authority, reported_on: reportedOn, reference: reference || null, note: note || null }),
      },
      true
    ),
};

// ── Moderation decisions (statements of reasons) ─────────────────────────────

export interface ModerationDecision {
  id: string;
  target_type: "post" | "comment" | "user" | "message";
  // removed: deleted by the moderation team; hidden / flagged: automatic,
  // after a report, until reviewed. warning / suspended / banned: account
  // measures (target_type "user").
  restriction: "removed" | "hidden" | "flagged" | "warning" | "suspended" | "banned";
  automated: boolean;
  reason: ReportReason;
  ground_type: "illegal" | "terms";
  ground: string;
  explanation: string;
  content_excerpt: string | null;
  // Length of a temporary suspension.
  suspension_days: number | null;
  created_at: string;
  lifted_at: string | null;
  superseded: boolean;
  // The decision that replaced this one (a removal after an automatic
  // hide), whose statement says what applies now.
  superseded_by: string | null;
  objection: string | null;
  objected_at: string | null;
  // superseded: a removal replaced the restriction objected to;
  // withdrawn: the author deleted their account.
  objection_status: "pending" | "rejected" | "accepted" | "superseded" | "withdrawn" | null;
  objection_response: string | null;
  objection_resolved_at: string | null;
  can_object: boolean;
  // What it followed; null for account measures from the standing page.
  source: "notice" | "own_initiative" | "authority_order" | "rights_claim" | null;
  // A removal whose content has been deleted for good: an accepted
  // objection can no longer restore it.
  content_purged: boolean;
}

export type ReportOutcome = "removed" | "account_measure" | "no_violation" | "obsolete" | "duplicate";

export interface MyReport {
  id: string;
  target_type: ReportTargetType;
  reason: ReportReason;
  status: "pending" | "dismissed" | "actioned" | "obsolete";
  outcome: ReportOutcome | null;
  created_at: string;
  reviewed_at: string | null;
  recheck_requested_at: string | null;
  // Dismissed within the last six months and not re-checked yet.
  can_recheck: boolean;
}

export interface AdminModerationDecision extends ModerationDecision {
  target_id: string;
  affected_username: string | null;
  delivered_at: string | null;
  // A held-back statement waiting for more than a week.
  overdue: boolean;
  // Reports still pending behind an automatic restriction: its objection
  // is answered by deciding them in the report queue.
  pending_reports: number;
}

// One row of the decision log (admin).
export interface LoggedDecision extends ModerationDecision {
  target_id: string;
  affected_username: string | null;
  decided_by_username: string | null;
  delivered_at: string | null;
  report_count: number;
  violation_type: string | null;
}

// What is waiting for an admin (the badges in Settings).
export interface AdminAttention {
  reports: number;
  urgent_reports: number;
  overdue_reports: number;
  objections: number;
  rights_claims: number;
  held_statements: number;
  overdue_held_statements: number;
  overdue_evidence: number;
  authority_reports: number;
  total: number;
}

export const moderationApi = {
  myDecisions: () => request<ModerationDecision[]>("/moderation/decisions", {}, true),
  decision: (id: string) => request<ModerationDecision>(`/moderation/decisions/${id}`, {}, true),
  object: (id: string, text: string) =>
    request<ModerationDecision>(
      `/moderation/decisions/${id}/objection`,
      { method: "POST", body: JSON.stringify({ text }) },
      true
    ),
  myReports: () => request<MyReport[]>("/moderation/reports", {}, true),
  // Once per dismissed report: back into the queue for another look.
  recheck: (reportId: string, note?: string) =>
    request<MyReport>(
      `/moderation/reports/${reportId}/recheck`,
      { method: "POST", body: JSON.stringify({ note: note || null }) },
      true
    ),
};

// ── Notices about illegal content (public form, DSA Art. 16) ─────────────────

export interface NewNotice {
  reason: ReportReason;
  explanation: string;
  content_url: string;
  // Optional only for child sexual abuse material.
  notifier_name: string | null;
  notifier_email: string | null;
  good_faith: boolean;
  // Honeypot; always empty from the real form.
  website: string;
}

export interface NoticeStatus {
  id: string;
  reason: ReportReason;
  content_url: string;
  explanation: string;
  created_at: string;
  decided_at: string | null;
  outcome: ReportOutcome | null;
}

export const noticesApi = {
  create: (notice: NewNotice) =>
    request<{ id: string; token: string }>("/notices", { method: "POST", body: JSON.stringify(notice) }, true),
  status: (id: string, token: string) =>
    request<NoticeStatus>(`/notices/${id}/status`, { method: "POST", body: JSON.stringify({ token }) }),
};

// ── Rights claims (copyright etc.) ───────────────────────────────────────────

export type RightsClaimType = "copyright" | "trademark" | "other";
export type RightsClaimStatus = "submitted" | "triaged" | "evidence_requested" | "accepted" | "declined" | "restored";

export interface NewRightsClaim {
  claim_type: RightsClaimType;
  claimant_name: string;
  claimant_email: string;
  claimant_organization: string | null;
  represented_party: string | null;
  content_url: string;
  work_description: string;
  ownership_basis: string;
  original_url: string | null;
  good_faith: boolean;
  // Honeypot, always empty from the real form.
  website: string;
}

export interface RightsClaimStatusView {
  id: string;
  claim_type: RightsClaimType;
  content_url: string;
  work_description: string;
  status: RightsClaimStatus;
  evidence_request: string | null;
  claimant_response: string | null;
  decision_reason: string | null;
  created_at: string;
  decided_at: string | null;
}

export interface AdminRightsClaim {
  id: string;
  created_at: string;
  claim_type: RightsClaimType;
  claimant_name: string;
  claimant_email: string;
  claimant_organization: string | null;
  represented_party: string | null;
  claimant_username: string | null;
  content_url: string;
  target_id: string;
  target_exists: boolean;
  target_username: string | null;
  target_caption: string | null;
  work_description: string;
  ownership_basis: string;
  original_url: string | null;
  status: RightsClaimStatus;
  evidence_request: string | null;
  claimant_response: string | null;
  decision_reason: string | null;
  decided_at: string | null;
  decision_id: string | null;
  overdue: boolean;
}

export const rightsApi = {
  // Public: works with or without a session (a signed-in claimant is linked).
  create: (claim: NewRightsClaim) =>
    request<{ id: string; token: string }>("/rights-claims", { method: "POST", body: JSON.stringify(claim) }),
  // The token goes in the body, never in a URL.
  status: (id: string, token: string) =>
    request<RightsClaimStatusView>(`/rights-claims/${id}/status`, { method: "POST", body: JSON.stringify({ token }) }),
  respond: (id: string, token: string, response: string) =>
    request<RightsClaimStatusView>(`/rights-claims/${id}/respond`, {
      method: "POST",
      body: JSON.stringify({ token, response }),
    }),
};

export const adminRightsApi = {
  list: (all = false) => request<AdminRightsClaim[]>(`/admin/rights-claims?filter=${all ? "all" : "open"}`, {}, true),
  triage: (id: string) => request<void>(`/admin/rights-claims/${id}/triage`, { method: "POST", body: "{}" }, true),
  requestEvidence: (id: string, message: string) =>
    request<void>(`/admin/rights-claims/${id}/request-evidence`, { method: "POST", body: JSON.stringify({ message }) }, true),
  accept: (id: string) => request<void>(`/admin/rights-claims/${id}/accept`, { method: "POST", body: "{}" }, true),
  decline: (id: string, message: string) =>
    request<void>(`/admin/rights-claims/${id}/decline`, { method: "POST", body: JSON.stringify({ message }) }, true),
};

// ── Notices about changed Terms / privacy policy ────────────────────────────

export type LegalDocument = "terms" | "privacy";

// A notice the signed-in user hasn't seen or accepted yet. Terms changes
// (requires_acceptance) must be accepted to keep using Klar.
export interface LegalUpdate {
  id: string;
  published_at: string;
  documents: LegalDocument[];
  summary: string;
  requires_acceptance: boolean;
}

export interface AdminLegalUpdate extends LegalUpdate {
  // The file in klar-web/legal-updates it came from; null if published by hand.
  source_key: string | null;
  emails_sent: number;
  emails_finished_at: string | null;
  // Accounts that existed when it was published, and how many of them
  // have seen or accepted it.
  audience: number;
  acknowledged: number;
}

export const legalUpdatesApi = {
  pending: () => request<LegalUpdate[]>("/legal-updates/pending", {}, true),
  acknowledge: (id: string) =>
    request<void>(`/legal-updates/${id}/acknowledge`, { method: "POST", body: "{}" }, true),
};

// Notices are published by the frontend deploy from klar-web/legal-updates;
// this only lists them with their progress.
export const adminLegalUpdatesApi = {
  list: () => request<AdminLegalUpdate[]>("/admin/legal-updates", {}, true),
};

// ── Official accounts (admin) ────────────────────────────────────────────────

// Accounts with a verified @klarsocial.eu address. Only an admin can rename
// them, and only they may take staff names like "Klar"; every rename is
// logged with its reason.
export interface OfficialAccount {
  id: string;
  username: string;
  email: string;
  created_at: string;
}

export interface OfficialRename {
  id: string;
  // null once the account is deleted
  user_id: string | null;
  old_username: string;
  new_username: string;
  reason: string;
  renamed_at: string;
  // the admin's current username; null once their account is deleted
  renamed_by: string | null;
}

export const adminOfficialAccountsApi = {
  list: () =>
    request<{ accounts: OfficialAccount[]; renames: OfficialRename[] }>("/admin/official-accounts", {}, true),
  rename: (id: string, username: string, reason: string) =>
    request<void>(
      `/admin/official-accounts/${id}/username`,
      { method: "POST", body: JSON.stringify({ username, reason }) },
      true,
    ),
};

// ── Account reviews (admin) ──────────────────────────────────────────────────

// Signals computed from existing activity (no IPs or devices): the biggest
// burst of posts/comments/messages in 10 minutes and of identical texts in
// the last 7 days, links, activity in the last day, and whether that came
// after 60+ days of silence.
export interface ReviewSignals {
  max_burst: number;
  max_duplicates: number;
  links: number;
  activity_24h: number;
  woke_up: boolean;
}

export type ReviewFlag = "burst" | "duplicates" | "woke_up";

export interface ReviewCandidate {
  username: string;
  signals: ReviewSignals;
  flags: ReviewFlag[];
  // Pending spam, fraud or impersonation reports on the account or its content.
  spam_reports: number;
  locked: boolean;
  last_review: string | null;
}

export interface AccountReview {
  review_id: string;
  overview: {
    username: string;
    display_name: string | null;
    created_at: string;
    email_verified: boolean;
    is_private: boolean;
    post_count: number;
    follower_count: number;
    following_count: number;
  };
  signals: ReviewSignals;
  flags: ReviewFlag[];
  standing_score: number;
  suspended: boolean;
  locked: boolean;
  posts: { id: string; caption: string | null; created_at: string; moderation_status: string; image_count: number }[];
  comments: { id: string; body: string; created_at: string; post_id: string; post_author: string | null }[];
  likes: { post_id: string; post_author: string | null; created_at: string }[];
  follows: { username: string; created_at: string }[];
  // Direct messages as numbers only, never their content.
  messages: { sent_24h: number; recipients_24h: number; sent_7d: number; recipients_7d: number };
  reports: { reason: ReportReason; target_type: ReportTargetType; status: string; details: string | null; created_at: string }[];
  history: { kind: "decision" | "lock" | "review"; what: string | null; at: string }[];
}

export type ReviewOutcome = "no_action" | "lock" | "bot";

export const adminReviewApi = {
  candidates: () => request<ReviewCandidate[]>("/admin/review-candidates", {}, true),
  // Opening is recorded (who, when, why): a reason is required.
  open: (username: string, reason: string, reportId?: string) =>
    request<AccountReview>(
      `/admin/users/${encodeURIComponent(username)}/review`,
      { method: "POST", body: JSON.stringify({ reason, report_id: reportId ?? null }) },
      true
    ),
  decide: (reviewId: string, outcome: ReviewOutcome, note: string) =>
    request<{ outcome: string }>(
      `/admin/reviews/${reviewId}/decide`,
      { method: "POST", body: JSON.stringify({ outcome, note }) },
      true
    ),
};

// ── Account locks (suspected takeovers, admin) ───────────────────────────────

// One lock = one incident record: why it was locked, the assessment
// (what the intruder could see, risk, reported to the authority or not),
// and how it ended.
export interface AccountLock {
  id: string;
  // null once the account is deleted.
  username: string | null;
  locked_by: string | null;
  locked_at: string;
  note: string;
  assessment: string | null;
  assessed_at: string | null;
  links_sent: number;
  last_link_sent_at: string;
  unlocked_at: string | null;
  unlocked_via: "password_reset" | "admin" | null;
}

export const adminLocksApi = {
  list: () => request<AccountLock[]>("/admin/locks", {}, true),
  lock: (username: string, note: string) =>
    request<void>(
      `/admin/users/${encodeURIComponent(username)}/lock`,
      { method: "POST", body: JSON.stringify({ note }) },
      true
    ),
  unlock: (id: string) => request<void>(`/admin/locks/${id}/unlock`, { method: "POST", body: "{}" }, true),
  assess: (id: string, assessment: string) =>
    request<void>(`/admin/locks/${id}`, { method: "PATCH", body: JSON.stringify({ assessment }) }, true),
};

export interface DecisionLogFilter {
  restriction?: string;
  reason?: string;
  source?: string;
  automated?: boolean;
  decided_by?: string;
  affected?: string;
  before_time?: string;
  before_id?: string;
  limit?: number;
}

export const adminModerationApi = {
  queue: () =>
    request<{ held: AdminModerationDecision[]; objections: AdminModerationDecision[] }>("/admin/moderation", {}, true),
  attention: () => request<AdminAttention>("/admin/attention", {}, true),
  decisions: (filter: DecisionLogFilter) => {
    const query = new URLSearchParams();
    for (const [key, value] of Object.entries(filter)) {
      if (value !== undefined && value !== "") query.set(key, String(value));
    }
    return request<LoggedDecision[]>(`/admin/decisions?${query}`, {}, true);
  },
  release: (id: string) =>
    request<void>(`/admin/moderation/decisions/${id}/release`, { method: "POST", body: "{}" }, true),
  resolveObjection: (id: string, accept: boolean, response: string) =>
    request<void>(
      `/admin/moderation/decisions/${id}/objection`,
      { method: "POST", body: JSON.stringify({ accept, response }) },
      true
    ),
};

// ── Post endpoints ────────────────────────────────────────────────────────────

export const posts = {
  feed: (cursor?: PostCursor, limit = 20) => {
    const params = new URLSearchParams({ limit: String(limit) });
    if (cursor) {
      params.set("cursor", cursor.time);
      params.set("cursor_id", cursor.id);
    }
    return request<Post[]>(`/feed?${params}`, {}, true);
  },

  discoveryFeed: (cursor?: PostCursor, limit = 15) => {
    const params = new URLSearchParams({ limit: String(limit) });
    if (cursor) {
      params.set("cursor_time", cursor.time);
      params.set("cursor_id", cursor.id);
    }
    return request<DiscoveryFeedResponse>(`/feed/discovery?${params}`, {}, true);
  },

  get: (id: string) => request<Post>(`/posts/${id}`),

  userPosts: (username: string, cursor?: PostCursor, limit = 20) => {
    const params = new URLSearchParams({ limit: String(limit) });
    if (cursor) {
      params.set("cursor", cursor.time);
      params.set("cursor_id", cursor.id);
    }
    return request<Post[]>(`/users/${username}/posts?${params}`);
  },

  media: (postId: string) =>
    request<MediaAsset[]>(`/posts/${postId}/media`),

  upload: (file: File, caption: string) => {
    const form = new FormData();
    form.append("image", file);
    if (caption) form.append("caption", caption);
    return request<unknown>("/posts/upload", { method: "POST", body: form }, true);
  },

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
  // Single-use, 30-second ticket for opening the SSE stream. EventSource
  // can't send headers, and an access token in the URL would end up in
  // CDN logs -- see the backend's create_stream_ticket.
  streamTicket: () =>
    request<{ ticket: string }>("/notifications/stream-ticket", { method: "POST" }, true),
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

// Shown in place of a chat participant who deleted their account (the
// conversation stays for the other person; see migration 20260929000100).
export const DELETED_USER_LABEL = "Deleted User";

export interface Conversation {
  id: string;
  // Both null when the other participant deleted their account.
  other_user_id: string | null;
  other_username: string | null;
  other_avatar_url: string | null;
  // Whichever is more recent: the last message, or the last reaction on
  // any message in the conversation. null only for a brand new
  // conversation with no activity yet.
  last_activity_kind: 'message' | 'reply' | 'reaction' | null;
  last_activity_actor_id: string | null;
  // Who wrote the message involved -- same as actor_id for
  // 'message'/'reply', but for 'reaction' this is who wrote the message
  // being reacted to (may differ from who reacted).
  last_activity_message_sender_id: string | null;
  last_activity_text: string | null;
  // Only set when last_activity_kind is 'reaction'.
  last_activity_emoji: string | null;
  updated_at: string;
}

export interface ChatMessage {
  id: string;
  conversation_id: string;
  // null when the sender deleted their account.
  sender_id: string | null;
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

// ── Feedback ──────────────────────────────────────────────────────────────────

export type FeedbackCategory = "bug" | "idea" | "other";

export interface FeedbackEntry {
  id: string;
  // null once the sender's account is deleted.
  username: string | null;
  category: FeedbackCategory;
  message: string;
  page_path: string | null;
  user_agent: string | null;
  viewport: string | null;
  status: "new" | "seen" | "done";
  admin_note: string | null;
  created_at: string;
  screenshots: { id: string; width: number; height: number }[];
}

/** Matches the backend's limits (handlers/feedback.rs). */
export const FEEDBACK_MAX_SCREENSHOTS = 3;
export const FEEDBACK_MAX_SCREENSHOT_BYTES = 10 * 1024 * 1024;

export const feedbackApi = {
  send: (
    category: FeedbackCategory,
    message: string,
    context: { page_path: string; user_agent: string; viewport: string } | null,
    screenshots: File[] = []
  ) => {
    const form = new FormData();
    form.append("category", category);
    form.append("message", message);
    for (const [key, value] of Object.entries(context ?? {})) form.append(key, value);
    for (const file of screenshots) form.append("screenshot", file);
    return request<void>("/feedback", { method: "POST", body: form }, true);
  },
  // Screenshots are only served to admins through the API, so they're
  // fetched with the session and shown from an object URL; the caller
  // revokes it.
  screenshotUrl: async (id: string): Promise<string> => {
    const res = await apiFetch(`/admin/feedback/screenshots/${id}`, {}, true);
    if (!res.ok) throw new Error(errorMessage(await parseBody(res), res.status));
    return URL.createObjectURL(await res.blob());
  },
  list: (all = false) => request<FeedbackEntry[]>(`/admin/feedback?filter=${all ? "all" : "open"}`, {}, true),
  update: (id: string, status: FeedbackEntry["status"], adminNote: string | null) =>
    request<void>(
      `/admin/feedback/${id}`,
      { method: "PATCH", body: JSON.stringify({ status, admin_note: adminNote }) },
      true
    ),
};
