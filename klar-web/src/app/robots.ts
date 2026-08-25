import type { MetadataRoute } from "next";

// Public marketing/legal pages are crawlable. Everything auth-gated or
// app-internal (feed, settings, admin, api, user content) is not — in line
// with Klar's privacy-first positioning, nothing user-generated is indexed
// by default. Revisit /users/[username] specifically if public profile
// discovery becomes an intentional product decision later.
export default function robots(): MetadataRoute.Robots {
  return {
    rules: [
      {
        userAgent: "*",
        allow: [
          "/welcome",
          "/impressum",
          "/datenschutz",
          "/nutzungsbedingungen",
          "/transparenz",
        ],
        disallow: [
          "/feed",
          "/settings",
          "/api",
          "/users",
          "/posts",
          "/search",
          "/chats",
          "/follow-requests",
          "/login",
          "/register",
          "/forgot-password",
          "/reset-password",
          "/verify-email",
          "/resend-verification",
        ],
      },
    ],
    sitemap: "https://www.klarsocial.eu/sitemap.xml",
  };
}
