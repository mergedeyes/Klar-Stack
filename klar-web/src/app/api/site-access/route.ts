import { NextRequest, NextResponse } from "next/server";

/**
 * POST /api/site-access -- verifies the site-wide passcode (see
 * middleware.ts) and, on success, sets the httpOnly cookie the
 * middleware checks on every subsequent request. This is a coming-soon
 * gate, not a real per-user auth system -- one shared passcode, no
 * accounts involved -- so a plain string comparison against a
 * server-only env var is proportionate here; SITE_ACCESS_PASSCODE is
 * never exposed to the client bundle since it isn't NEXT_PUBLIC_-prefixed
 * and this route only ever runs server-side.
 */
export async function POST(req: NextRequest) {
  const expectedPasscode = process.env.SITE_ACCESS_PASSCODE;

  if (!expectedPasscode) {
    return NextResponse.json(
      { error: "Site access gate is not configured" },
      { status: 500 }
    );
  }

  const body = await req.json().catch(() => null);
  const passcode = typeof body?.passcode === "string" ? body.passcode : "";

  if (passcode !== expectedPasscode) {
    return NextResponse.json({ error: "Incorrect passcode" }, { status: 401 });
  }

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
