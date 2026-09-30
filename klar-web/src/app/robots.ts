import type { MetadataRoute } from "next";

// Public marketing/legal pages are crawlable. Everything auth-gated or
// app-internal (feed, settings, admin, api, user content) is not — in line
// with Klar's privacy-first positioning, nothing user-generated is indexed
// by default. Revisit /users/[username] specifically if public profile
// discovery becomes an intentional product decision later.
//
// Link-preview bots get /posts/ as well, so a shared post shows its preview.
// Some honour robots.txt (Twitter/X's does), and a bot matching its own group
// ignores the "*" group, so they get the full rules again with /posts/ added.
// The post pages carry noindex, so search engines still don't list them.
const PUBLIC_PAGES = [
  "/welcome",
  "/impressum",
  "/datenschutz",
  "/nutzungsbedingungen",
  "/transparenz",
];

const PRIVATE_PAGES = [
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
];

const PREVIEW_BOTS = [
  "facebookexternalhit",
  "Twitterbot",
  "WhatsApp",
  "TelegramBot",
  "Slackbot-LinkExpanding",
  "Discordbot",
  "LinkedInBot",
];

export default function robots(): MetadataRoute.Robots {
  return {
    rules: [
      { userAgent: "*", allow: PUBLIC_PAGES, disallow: PRIVATE_PAGES },
      {
        userAgent: PREVIEW_BOTS,
        allow: [...PUBLIC_PAGES, "/posts/"],
        disallow: PRIVATE_PAGES.filter((path) => path !== "/posts"),
      },
    ],
    sitemap: "https://www.klarsocial.eu/sitemap.xml",
  };
}
