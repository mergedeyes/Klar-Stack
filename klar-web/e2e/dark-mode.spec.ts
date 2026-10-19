import { expect, test, type Page } from "@playwright/test";
import { signIn, signUp } from "./helpers";

// Klar follows the device's light or dark setting, and the browser's bars
// (theme-color) match the page, so the status bar never looks like a black
// bar over a light page or the other way round.

async function pageColours(page: Page) {
  return page.evaluate(() => {
    const scheme = matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
    const meta = [...document.querySelectorAll<HTMLMetaElement>('meta[name="theme-color"]')].find((m) => matchMedia(m.media || "all").matches);
    // The colours are defined in oklch, which Chromium reports as lab();
    // painting one pixel gives the sRGB value the screen shows.
    const ctx = document.createElement("canvas").getContext("2d")!;
    ctx.fillStyle = getComputedStyle(document.body).backgroundColor;
    ctx.fillRect(0, 0, 1, 1);
    const [r, g, b] = ctx.getImageData(0, 0, 1, 1).data;
    return { scheme, body: `rgb(${r}, ${g}, ${b})`, themeColor: meta?.content };
  });
}

for (const [scheme, body, themeColor] of [
  ["light", "rgb(245, 245, 245)", "#f5f5f5"],
  ["dark", "rgb(10, 10, 10)", "#0a0a0a"],
] as const) {
  test(`in ${scheme} mode the page and the status bar are ${scheme}`, async ({ page }) => {
    await page.emulateMedia({ colorScheme: scheme });
    await signIn(page, await signUp(scheme));
    await page.goto("/feed");
    await expect(page.locator("main")).toBeVisible();
    expect(await pageColours(page)).toEqual({ scheme, body, themeColor });
  });
}
