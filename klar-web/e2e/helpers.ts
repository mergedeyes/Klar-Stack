import fs from "node:fs";
import path from "node:path";
import { expect, type APIRequestContext, type Page } from "@playwright/test";

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
function clientIp(): string {
  const n = () => Math.floor(Math.random() * 250) + 1;
  return `10.${n()}.${n()}.${n()}`;
}

export async function signUp(request: APIRequestContext, prefix = "user"): Promise<Session> {
  const username = uniqueName(prefix);
  const res = await request.post(`${API}/auth/register`, {
    headers: { "X-Forwarded-For": clientIp() },
    data: { username, email: `${username}@example.test`, password: "test-password-123", accept_terms: true },
  });
  expect(res.ok(), await res.text()).toBeTruthy();
  const body = await res.json();
  return { id: body.user.id, username, access_token: body.access_token, refresh_token: body.refresh_token };
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
