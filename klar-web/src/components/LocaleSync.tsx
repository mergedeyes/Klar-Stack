"use client";

import { useEffect } from "react";
import { useRouter } from "next/navigation";
import { useLocale } from "next-intl";
import { useAuth } from "@/lib/auth-context";
import { users } from "@/lib/api";
import { isLocale } from "@/i18n/config";
import { forgetLocale, rememberLocale } from "@/i18n/cookie";

// Keeps the page in the signed-in account's language. The server only sees
// the browser's language (the session lives in localStorage), so the
// account's goes into a cookie it can read, and the page is rendered again
// when the two differ. An account without one yet (created before
// languages existed) gets the one the page is shown in. Signed out, the
// cookie goes, and pages follow the browser again.
export default function LocaleSync() {
  const { user, loading } = useAuth();
  const locale = useLocale();
  const router = useRouter();

  useEffect(() => {
    if (loading) return;
    if (!user) {
      if (forgetLocale()) router.refresh();
      return;
    }
    if (!isLocale(user.locale)) {
      users.setLocale(locale).catch(() => {
        // Tried again on the next visit.
      });
      rememberLocale(locale);
      return;
    }
    rememberLocale(user.locale);
    if (user.locale !== locale) router.refresh();
  }, [user, loading, locale, router]);

  return null;
}
