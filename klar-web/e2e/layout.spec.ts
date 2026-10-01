import { expect, test } from "@playwright/test";
import { measure, signIn, signUp } from "./helpers";

// The footer sits on the screen's bottom edge: short pages fill exactly
// one screen (no scrolling just to reach the footer), long pages scroll
// with the footer pinned, and chats fits the screen with its own scrolling.

for (const path of ["/login", "/register"]) {
  test(`${path} fits the screen with the footer at the bottom`, async ({ page }) => {
    await page.goto(path);
    await expect(page.locator("footer")).toBeVisible();
    const m = await measure(page);
    expect(m.pageHeight).toBe(m.screenHeight);
    expect(m.footerBottom).toBe(m.screenHeight);
    expect(m.horizontalScroll).toBe(false);
  });
}

for (const path of ["/settings", "/settings/discovery", "/chats", "/moderation"]) {
  test(`${path} (signed in) fits the screen`, async ({ page }) => {
    await signIn(page, await signUp());
    await page.goto(path);
    await expect(page.locator("footer")).toBeVisible();
    const m = await measure(page);
    expect(m.pageHeight).toBe(m.screenHeight);
    expect(m.footerBottom).toBe(m.screenHeight);
  });
}

test("a long page scrolls with the footer pinned to the bottom", async ({ page }) => {
  await page.goto("/datenschutz");
  const top = await measure(page);
  expect(top.pageHeight).toBeGreaterThan(top.screenHeight);
  expect(top.footerBottom).toBe(top.screenHeight);

  await page.evaluate(() => window.scrollTo(0, document.documentElement.scrollHeight));
  const end = await measure(page);
  expect(end.footerBottom).toBe(end.screenHeight);
});
