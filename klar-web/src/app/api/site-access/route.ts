import { timingSafeEqual } from "node:crypto";
import { NextRequest, NextResponse } from "next/server";

// Brute-force protection: at most MAX_FAILURES wrong guesses per client IP
// per WINDOW_MS, plus a fixed delay on every wrong guess. In-memory, so
// it's per-replica and resets on restart -- proportionate for a shared
// test passcode, not a real auth system.
const MAX_FAILURES = 10;
const WINDOW_MS = 15 * 60 * 1000;
const FAILURE_DELAY_MS = 1000;

const failures = new Map<string, { count: number; resetAt: number }>();

// Same rule as the backend's rate_limit.rs: X-Forwarded-For entries are
// appended by each proxy, so only the Nth-from-right entry (N =
// TRUSTED_PROXY_HOPS, default 1) is trustworthy -- anything left of it is
// client-supplied and could be rotated per request to dodge the limit.
function clientIp(req: NextRequest): string {
  const hops = Number(process.env.TRUSTED_PROXY_HOPS ?? "1");
  const entries = (req.headers.get("x-forwarded-for") ?? "")
    .split(",")
    .map((s) => s.trim())
    .filter(Boolean);
  if (hops <= 0 || entries.length === 0) return "unknown";
  return entries[Math.max(entries.length - hops, 0)];
}

// Constant-time comparison. timingSafeEqual needs equal-length buffers, so
// both sides are zero-padded to the longer length instead of returning
// early on a length mismatch (which would leak the passcode's length).
// The explicit length check is still required, or "abc" would match a
// passcode of "abc\0"; it's combined with & so both always run.
function passcodeMatches(given: string, expected: string): boolean {
  const givenBuf = Buffer.from(given, "utf8");
  const expectedBuf = Buffer.from(expected, "utf8");
  const len = Math.max(givenBuf.length, expectedBuf.length);

  const a = Buffer.alloc(len);
  const b = Buffer.alloc(len);
  givenBuf.copy(a);
  expectedBuf.copy(b);

  const sameBytes = timingSafeEqual(a, b);
  const sameLength = givenBuf.length === expectedBuf.length;
  return sameBytes && sameLength;
}

/**
 * POST /api/site-access -- verifies the site-wide passcode (see
 * proxy.ts) and, on success, sets the httpOnly cookie the proxy checks
 * on every subsequent request. This is a coming-soon gate, not a real
 * per-user auth system -- one shared passcode, no accounts involved.
 * SITE_ACCESS_PASSCODE is never exposed to the client bundle since it
 * isn't NEXT_PUBLIC_-prefixed and this route only ever runs server-side.
 */
export async function POST(req: NextRequest) {
  const expectedPasscode = process.env.SITE_ACCESS_PASSCODE;

  if (!expectedPasscode) {
    return NextResponse.json(
      { error: "Site access gate is not configured" },
      { status: 500 }
    );
  }

  const ip = clientIp(req);
  const now = Date.now();
  let entry = failures.get(ip);
  if (entry && entry.resetAt <= now) {
    failures.delete(ip);
    entry = undefined;
  }

  if (entry && entry.count >= MAX_FAILURES) {
    const retryAfter = Math.ceil((entry.resetAt - now) / 1000);
    return NextResponse.json(
      { error: "Too many attempts. Please try again later." },
      { status: 429, headers: { "Retry-After": String(retryAfter) } }
    );
  }

  const body = await req.json().catch(() => null);
  const passcode = typeof body?.passcode === "string" ? body.passcode : "";

  if (!passcodeMatches(passcode, expectedPasscode)) {
    if (entry) entry.count += 1;
    else failures.set(ip, { count: 1, resetAt: now + WINDOW_MS });

    // Opportunistic cleanup so the map can't grow without bound.
    if (failures.size > 10_000) {
      for (const [key, value] of failures) {
        if (value.resetAt <= now) failures.delete(key);
      }
    }

    await new Promise((resolve) => setTimeout(resolve, FAILURE_DELAY_MS));
    return NextResponse.json({ error: "Incorrect passcode" }, { status: 401 });
  }

  failures.delete(ip);

  const res = NextResponse.json({ ok: true });
  res.cookies.set("klar_gate", expectedPasscode, {
    httpOnly: true,
    secure: process.env.NODE_ENV === "production",
    sameSite: "lax",
    path: "/",
    maxAge: 60 * 60 * 24 * 90, // 90 days
  });
  return res;
}
