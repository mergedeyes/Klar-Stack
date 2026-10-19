import { expect, test } from "@playwright/test";
import { apiGet, signIn, signUp } from "./helpers";

// The page's language: the browser's while signed out, the account's once
// signed in, and changed under Settings.

test("signed out, pages follow the browser's language", async ({ browser }) => {
  const context = await browser.newContext({ locale: "de-DE" });
  const page = await context.newPage();
  await page.goto("/login");
  await expect(page.locator("html")).toHaveAttribute("lang", "de");
  await context.close();
});

test("the account's language is kept and changed in Settings", async ({ page, request }) => {
  const session = await signUp("language");
  await signIn(page, session);
  await page.goto("/settings");
  // An account from before languages existed gets the page's.
  await expect.poll(async () => (await apiGet(request, session, "/users/me")).locale).toBe("en");

  await page.getByRole("link", { name: /Language and app icon/ }).click();
  await page.getByLabel("Language").selectOption("de");
  await expect(page.getByRole("heading", { name: "Sprache und App-Symbol" })).toBeVisible();
  await expect(page.locator("html")).toHaveAttribute("lang", "de");
  expect((await apiGet(request, session, "/users/me")).locale).toBe("de");

  // Still German after a reload, though the browser asks for English.
  await page.reload();
  await expect(page.getByRole("heading", { name: "Sprache und App-Symbol" })).toBeVisible();

  await page.getByLabel("Sprache").selectOption("en");
  await expect(page.getByRole("heading", { name: "Language and app icon" })).toBeVisible();
});
