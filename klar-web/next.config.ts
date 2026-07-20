import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  images: {
    unoptimized: process.env.NODE_ENV === 'development',
    remotePatterns: [
      {
        protocol: "http",
        hostname: "localhost",
        port: "3000",
      },
      // Hier ist der neue Eintrag für dein Bunny CDN
      {
        protocol: "https",
        hostname: "cdn.klarsocial.eu",
      },
    ],
  },
};

export default nextConfig;