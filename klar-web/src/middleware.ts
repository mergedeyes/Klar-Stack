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

export function middleware(req: NextRequest) {
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
  // Everything except Next's own static/image optimization internals --
  // those are already excluded above too, but excluding them at the
  // matcher level means middleware doesn't even run for them at all.
  matcher: ["/((?!_next/static|_next/image).*)"],
};
