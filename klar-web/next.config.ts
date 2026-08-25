import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  output: "standalone",
  images: {
    unoptimized: process.env.NODE_ENV === "development",
    remotePatterns: [
      {
        protocol: "http",
        hostname: "localhost",
        port: "3000",
      },
      {
        protocol: "https",
        hostname: "cdn.klarsocial.eu",
      },
    ],
  },
  poweredByHeader: false, // kills X-Powered-By (10037)
  async headers() {
    return [
      {
        source: "/:path*",
        headers: [
          { key: "X-Frame-Options", value: "DENY" },                     // 10020
          { key: "X-Content-Type-Options", value: "nosniff" },           // 10021
          { key: "Strict-Transport-Security", value: "max-age=63072000; includeSubDomains; preload" }, // 10035
          { key: "Permissions-Policy", value: "camera=(), microphone=(), geolocation=()" }, // 10063
          { key: "Cross-Origin-Resource-Policy", value: "same-origin" }, // 90004
          { key: "Cross-Origin-Opener-Policy", value: "same-origin" },   // 90004 (COOP)
          // COEP is gated behind an env var: enabling it blocks any cross-origin
          // resource (img/video/link) fetched WITHOUT a `crossorigin` attribute
          // unless the origin sends Cross-Origin-Resource-Policy. Bunny CDN
          // currently returns Access-Control-Allow-Origin: * but no CORP header,
          // so this is only safe once confirmed nothing on the site hits
          // cdn.klarsocial.eu directly outside of next/image (which proxies
          // same-origin via /_next/image and is unaffected either way).
          ...(process.env.ENABLE_COEP === "true"
            ? [{ key: "Cross-Origin-Embedder-Policy", value: "require-corp" }] // 90004 (COEP)
            : []),
        ],
      },
    ];
  },
};

export default nextConfig;
