import { cookies } from "next/headers";
import { APP_ICON_COOKIE, appIcon } from "@/app-icons";
import AppearanceSettings from "@/components/settings/AppearanceSettings";

// Reads the device's app icon on the server, so the right one is ticked
// from the first render.
export default async function AppearancePage() {
  const current = appIcon((await cookies()).get(APP_ICON_COOKIE)?.value).id;
  return <AppearanceSettings currentIcon={current} />;
}
