import { expect, test, type Page } from "@playwright/test";
import { asHomeScreenApp, signIn, signUp } from "./helpers";

// The home screen app draws under the status bar and the gesture bar
// (viewport-fit=cover, components/EdgeToEdge.tsx); a browser tab doesn't,
// since there it would break Chrome's install sheet.

test.skip(({ isMobile }) => !isMobile, "a phone's home screen");

const viewport = (page: Page) => page.locator('meta[name="viewport"]');

test("a browser tab doesn't draw edge to edge", async ({ page }) => {
  await signIn(page, await signUp("tab"));
  await page.goto("/feed");
  await expect(page.locator("main")).toBeVisible();
  await expect(viewport(page)).not.toHaveAttribute("content", /viewport-fit=cover/);
});

test("the home screen app draws edge to edge, also after the layout renders again", async ({ page }) => {
  await asHomeScreenApp(page);
  await signIn(page, await signUp("app"));
  await page.goto("/feed");
  await expect(viewport(page)).toHaveAttribute("content", /viewport-fit=cover/);

  // A language change renders the root layout again.
  await page.goto("/settings/appearance");
  await page.getByLabel("Language").selectOption("de");
  await expect(page.getByRole("heading", { name: "Sprache und App-Symbol" })).toBeVisible();
  await expect(viewport(page)).toHaveAttribute("content", /viewport-fit=cover/);
});
