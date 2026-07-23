"use client";

import { useEffect } from "react";
import { useRouter } from "next/navigation";
import { useAuth } from "@/lib/auth-context";

/**
 * Root route — not a static landing page, just a redirect based on auth
 * state. Auth lives in localStorage/React state (not a cookie a server
 * component could read), so this has to be a client component that waits
 * for AuthProvider's initial restore-from-token check (`loading`) before
 * deciding where to send the user, same as the auth guard already used on
 * /feed and other protected pages.
 */
export default function Home() {
  const { user, loading } = useAuth();
  const router = useRouter();

  useEffect(() => {
    if (loading) return;
    router.replace(user ? "/feed" : "/login");
  }, [user, loading, router]);

  return null;
}
