import type en from "../messages/en.json";
import type { AppLocale } from "./i18n/config";

// Typed message keys: t("settings.titel") fails to compile instead of
// rendering the key.
declare module "next-intl" {
  interface AppConfig {
    Locale: AppLocale;
    Messages: typeof en;
  }
}
