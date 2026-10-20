import { useCallback } from "react";
import { useLocale, useTranslations } from "next-intl";

/** Short relative time ("5m", "3h", "2d"; "5 Min.", "3 Std.", "2 T."),
 *  then the date once a week old, in the page's language. */
export function useTimeAgo(): (dateStr: string) => string {
  const t = useTranslations("time");
  const locale = useLocale();
  return useCallback(
    (dateStr: string) => {
      const diff = Date.now() - new Date(dateStr).getTime();
      const mins = Math.floor(diff / 60000);
      if (mins < 1) return t("justNow");
      if (mins < 60) return t("minutes", { n: mins });
      const hrs = Math.floor(mins / 60);
      if (hrs < 24) return t("hours", { n: hrs });
      const days = Math.floor(hrs / 24);
      if (days < 7) return t("days", { n: days });
      return new Date(dateStr).toLocaleDateString(locale);
    },
    [t, locale],
  );
}
