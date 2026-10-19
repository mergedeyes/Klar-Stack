import { expect, test, type Page } from "@playwright/test";
import { signIn, signUp } from "./helpers";
import { APP_ICON_COOKIE, DEFAULT_APP_ICON } from "../src/app-icons";

// The app icon is chosen per device under Settings -> Language and app
// icon: the tab icon, the home screen icon and the manifest follow it.

function headLink(page: Page, rel: string) {
  return page.locator(`head link[rel="${rel}"]`);
}

test("choosing an app icon changes the tab, home screen and manifest icons", async ({ page, request }) => {
  await signIn(page, await signUp("icons"));
  await page.goto("/settings/appearance");

  const neon = page.getByRole("button", { name: "80s Neon" });
  const fallback = page.getByRole("button", { name: /^Phaser Violet/ });
  await expect(fallback).toHaveAttribute("aria-pressed", "true");
  await expect(headLink(page, "manifest")).toHaveAttribute("href", `/app-icons/${DEFAULT_APP_ICON}/manifest.webmanifest`);

  await neon.click();
  await expect(neon).toHaveAttribute("aria-pressed", "true");
  await expect(headLink(page, "manifest")).toHaveAttribute("href", "/app-icons/retro-80er-neon/manifest.webmanifest");
  await expect(headLink(page, "apple-touch-icon")).toHaveAttribute("href", "/app-icons/retro-80er-neon/apple.png");
  await expect(headLink(page, "icon")).toHaveAttribute("href", "/app-icons/retro-80er-neon/192.png");

  // Kept after a reload, in a cookie only.
  await page.reload();
  await expect(neon).toHaveAttribute("aria-pressed", "true");
  expect((await page.context().cookies()).find((c) => c.name === APP_ICON_COOKIE)?.value).toBe("retro-80er-neon");

  // The manifest names that icon's files, and they exist.
  const manifest = await (await request.get("/app-icons/retro-80er-neon/manifest.webmanifest")).json();
  expect(manifest.icons.map((i: { src: string }) => i.src)).toContain("/app-icons/retro-80er-neon/maskable.png");
  for (const icon of manifest.icons) expect((await request.get(icon.src)).status()).toBe(200);

  // Back to the default: the cookie goes.
  await fallback.click();
  await expect(headLink(page, "manifest")).toHaveAttribute("href", `/app-icons/${DEFAULT_APP_ICON}/manifest.webmanifest`);
  expect((await page.context().cookies()).find((c) => c.name === APP_ICON_COOKIE)).toBeUndefined();
});
