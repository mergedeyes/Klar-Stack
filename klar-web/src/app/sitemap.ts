import type { MetadataRoute } from "next";

const baseUrl = "https://www.klarsocial.eu";

// Only the public, unauthenticated pages — same set as robots.ts allows.
export default function sitemap(): MetadataRoute.Sitemap {
  const routes = [
    "/welcome",
    "/impressum",
    "/datenschutz",
    "/nutzungsbedingungen",
    "/transparenz",
  ];

  return routes.map((route) => ({
    url: `${baseUrl}${route}`,
    lastModified: new Date(),
  }));
}
