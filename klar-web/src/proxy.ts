import { NextRequest, NextResponse } from "next/server";

// Paths reachable without the passcode:
// - /welcome itself (the gate page) and its own API route, or you'd
//   never be able to reach the form that lets you in
// - the legal pages (Impressum/Datenschutz/Nutzungsbedingungen/
//   Transparenz) -- these need to stay public regardless of the gate,
//   both for legal-compliance reasons (an Impressum has to be reachable)
//   and because they're linked from the global Footer on every page,
//   including /welcome itself
const PUBLIC_PATHS = [
  "/welcome",
  "/api/site-access",
  "/impressum",
  "/datenschutz",
  "/nutzungsbedingungen",
  "/transparenz",
  "/favicon.ico",
];

// Next.js 16 renamed middleware.ts -> proxy.ts (exported function
// "proxy", not "middleware") -- the old convention is deprecated and, in
// practice, wasn't actually being invoked at all under 16.2.11. This is
// the same logic as before, just under the name Next.js 16 expects.
export function proxy(req: NextRequest) {
  const { pathname } = req.nextUrl;

  if (
    pathname.startsWith("/_next") ||
    PUBLIC_PATHS.some((p) => pathname === p || pathname.startsWith(`${p}/`))
  ) {
    return NextResponse.next();
  }

  const expectedPasscode = process.env.SITE_ACCESS_PASSCODE;

  // No passcode configured -- gate is effectively disabled (e.g. local
  // dev, where you generally don't want to bother setting this up).
  // Production deploys must set SITE_ACCESS_PASSCODE for the gate to
  // actually do anything.
  if (!expectedPasscode) {
    return NextResponse.next();
  }

  const cookieValue = req.cookies.get("klar_gate")?.value;

  if (cookieValue === expectedPasscode) {
    return NextResponse.next();
  }

  const url = req.nextUrl.clone();
  url.pathname = "/welcome";
  url.search = "";
  return NextResponse.redirect(url);
}

export const config = {
  matcher: ["/((?!_next/static|_next/image).*)"],
};
