import fs from "node:fs";
import path from "node:path";
import { expect, request as playwrightRequest, test, type APIRequestContext, type Browser, type Page } from "@playwright/test";
import pg from "pg";

// Test data is created through the API directly; the browser only sees
// the result. Every test makes its own users and posts, so tests don't
// depend on each other or on what's already in the database.

export const API = process.env.E2E_API_URL ?? "http://127.0.0.1:3000";

export interface Session {
  id: string;
  username: string;
  access_token: string;
  refresh_token: string;
}

let counter = 0;

/** Unique within a run and across runs on the same database. */
export function uniqueName(prefix: string): string {
  const suffix = `${Date.now().toString(36)}${(counter++).toString(36)}${Math.floor(Math.random() * 1296).toString(36)}`;
  return `${prefix}_${suffix}`.slice(0, 30);
}

/** Each client its own address, so the per-IP sign-up limit never trips. */
export function clientIp(): string {
  const n = () => Math.floor(Math.random() * 250) + 1;
  return `10.${n()}.${n()}.${n()}`;
}

/** Registers through a context of its own: sign-up sets auth cookies, and
 * the backend reads the cookie before the Authorization header, so a
 * shared context would make every later call act as the newest user.
 *
 * The address is marked verified in the test database, since posting,
 * commenting, messaging and reporting need that (the link only goes to an
 * inbox nobody reads); `verified: false` keeps the fresh, unverified state.
 * Without E2E_DATABASE_URL a test that needs a verified account skips. */
export async function signUp(prefix = "user", email?: string, { verified = true } = {}): Promise<Session> {
  if (verified) test.skip(!process.env.E2E_DATABASE_URL, "verifying the address needs E2E_DATABASE_URL (see e2e/README.md)");
  const username = uniqueName(prefix);
  const context = await playwrightRequest.newContext();
  try {
    const res = await context.post(`${API}/auth/register`, {
      headers: { "X-Forwarded-For": clientIp() },
      data: { username, email: email ?? `${username}@example.test`, password: "test-password-123", accept_terms: true },
    });
    expect(res.ok(), await res.text()).toBeTruthy();
    const body = await res.json();
    if (verified) {
      await withDb((db) => db.query("UPDATE users SET email_verified = TRUE WHERE id = $1", [body.user.id]));
    }
    return { id: body.user.id, username, access_token: body.access_token, refresh_token: body.refresh_token };
  } finally {
    await context.dispose();
  }
}

/** An account whose reports hide or flag content before review: verified
 * and older than a day (handlers/reports.rs). A fresh account's reports
 * only queue. */
export async function trustedSignUp(prefix = "reporter"): Promise<Session> {
  const session = await signUp(prefix);
  await withDb((db) => db.query("UPDATE users SET created_at = NOW() - INTERVAL '2 days' WHERE id = $1", [session.id]));
  return session;
}

function auth(session: Session) {
  return { Authorization: `Bearer ${session.access_token}`, "X-Forwarded-For": clientIp() };
}

/** Signs the browser in by seeding the tokens the app keeps in localStorage. */
export async function signIn(page: Page, session: Session) {
  await page.context().addInitScript(([access, refresh]) => {
    localStorage.setItem("klar_access_token", access);
    localStorage.setItem("klar_refresh_token", refresh);
  }, [session.access_token, session.refresh_token]);
}

export async function upload(
  request: APIRequestContext,
  session: Session,
  caption: string,
  image: "landscape" | "portrait" = "landscape",
): Promise<string> {
  const file = path.join(__dirname, "fixtures", `${image}.png`);
  const res = await request.post(`${API}/posts/upload`, {
    headers: auth(session),
    multipart: { caption, image: { name: `${image}.png`, mimeType: "image/png", buffer: fs.readFileSync(file) } },
  });
  expect(res.ok(), await res.text()).toBeTruthy();
  return (await res.json()).post.id;
}

export async function apiCall(
  request: APIRequestContext,
  session: Session,
  method: "POST" | "PATCH" | "DELETE",
  urlPath: string,
  data?: unknown,
) {
  const res = await request.fetch(`${API}${urlPath}`, { method, headers: auth(session), data });
  expect(res.ok(), await res.text()).toBeTruthy();
  return res;
}

/** Page height, screen height, and where the footer's bottom edge is. */
export async function measure(page: Page) {
  return page.evaluate(() => ({
    pageHeight: document.documentElement.scrollHeight,
    screenHeight: window.innerHeight,
    footerBottom: Math.round(document.querySelector("footer")!.getBoundingClientRect().bottom),
    horizontalScroll: document.documentElement.scrollWidth > window.innerWidth,
  }));
}

// ── Admin and database ────────────────────────────────────────────────────────

/** The admin the global setup registers; CI lists it in ADMIN_EMAILS. */
export const ADMIN_EMAIL = "e2e-admin@example.test";
export const ADMIN_USERNAME = "e2e_admin";
export const ADMIN_PASSWORD = "test-password-123";

/**
 * Runs `fn` with a connection to the test database, for what the API can't
 * do on purpose: verifying the admin, reading a reset token from an email
 * that goes nowhere, moving timestamps back. Only ever the throwaway test
 * database (E2E_DATABASE_URL).
 */
export async function withDb<T>(fn: (db: pg.Client) => Promise<T>): Promise<T> {
  const db = new pg.Client({ connectionString: process.env.E2E_DATABASE_URL });
  await db.connect();
  try {
    return await fn(db);
  } finally {
    await db.end();
  }
}

/** A fresh session for the admin. Skips the test without a test database. */
export async function adminSession(): Promise<Session> {
  test.skip(!process.env.E2E_DATABASE_URL, "needs E2E_DATABASE_URL (see e2e/README.md)");
  const context = await playwrightRequest.newContext();
  try {
    const res = await context.post(`${API}/auth/login`, {
      headers: { "X-Forwarded-For": clientIp() },
      data: { email: ADMIN_EMAIL, password: ADMIN_PASSWORD },
    });
    expect(res.ok(), await res.text()).toBeTruthy();
    const body = await res.json();
    return { id: body.user.id, username: ADMIN_USERNAME, access_token: body.access_token, refresh_token: body.refresh_token };
  } finally {
    await context.dispose();
  }
}

/** POST/PATCH through the API and return the JSON body. */
export async function apiJson<T = Record<string, unknown>>(
  request: APIRequestContext,
  session: Session,
  method: "POST" | "PATCH",
  urlPath: string,
  data: unknown = {},
): Promise<T> {
  const res = await apiCall(request, session, method, urlPath, data);
  const text = await res.text();
  return (text ? JSON.parse(text) : {}) as T;
}

export async function apiGet<T = Record<string, unknown>>(request: APIRequestContext, session: Session, urlPath: string): Promise<T> {
  const res = await request.get(`${API}${urlPath}`, { headers: auth(session) });
  expect(res.ok(), await res.text()).toBeTruthy();
  return (await res.json()) as T;
}

export async function comment(request: APIRequestContext, session: Session, postId: string, body: string): Promise<string> {
  return (await apiJson<{ id: string }>(request, session, "POST", `/posts/${postId}/comments`, { body })).id;
}

export async function report(
  request: APIRequestContext,
  session: Session,
  targetType: "post" | "comment" | "user",
  targetId: string,
  reason: string,
): Promise<string> {
  return (await apiJson<{ id: string }>(request, session, "POST", "/reports", { target_type: targetType, target_id: targetId, reason })).id;
}

/**
 * Has `author` write a comment on a post of `reporter`, reports it and has
 * the admin remove it, classified as `violation` (default: the reason's
 * first type). Returns the comment id.
 */
export async function removedComment(
  request: APIRequestContext,
  opts: { author: Session; reporter: Session; admin: Session; reason?: string; violation?: string; text?: string },
): Promise<string> {
  const post = await upload(request, opts.reporter, "a post");
  const id = await comment(request, opts.author, post, opts.text ?? "you suck");
  const reportId = await report(request, opts.reporter, "comment", id, opts.reason ?? "harassment");
  await apiCall(request, opts.admin, "POST", `/admin/reports/${reportId}/remove`, {
    violation: opts.violation ?? null,
    justification: opts.violation ? "e2e" : null,
  });
  return id;
}

/** Makes `a` and `b` follow each other (needed to message). */
export async function mutualFollow(request: APIRequestContext, a: Session, b: Session) {
  await apiCall(request, a, "POST", `/users/${b.username}/follow`, {});
  await apiCall(request, b, "POST", `/users/${a.username}/follow`, {});
}

/**
 * A page in a context of its own, signed in as `session`, with this
 * project's screen (desktop or phone) -- for a second person in the same
 * test, e.g. the admin next to the reported user. Confirm dialogs are
 * accepted.
 */
export async function pageFor(browser: Browser, session: Session): Promise<Page> {
  const { viewport, userAgent, deviceScaleFactor, isMobile, hasTouch } = test.info().project.use;
  const context = await browser.newContext({
    viewport,
    userAgent,
    deviceScaleFactor,
    isMobile,
    hasTouch,
    baseURL: process.env.E2E_BASE_URL ?? "http://localhost:3001",
  });
  const page = await context.newPage();
  page.on("dialog", (dialog) => dialog.accept());
  await signIn(page, session);
  return page;
}
