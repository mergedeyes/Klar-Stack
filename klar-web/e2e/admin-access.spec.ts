import { expect, test } from "@playwright/test";
import { adminSession, measure, pageFor, signIn, signUp } from "./helpers";

// Admin pages: closed to everyone else, listed in the admin's settings, and
// usable at phone width.

const ADMIN_PAGES = ["/admin/reports", "/admin/standing", "/admin/security", "/admin/moderation", "/admin/evidence"];

test("the admin pages refuse ordinary accounts", async ({ page }) => {
  await signIn(page, await signUp("nosy"));
  for (const path of ["/admin/standing", "/admin/security", "/admin/reports"]) {
    await page.goto(path);
    await expect(page.getByText("Admin access required")).toBeVisible();
  }
  await page.goto("/admin/review/someone");
  await page.getByLabel("Reason").fill("curious");
  await page.getByRole("button", { name: "Open review" }).click();
  await expect(page.getByText("Admin access required")).toBeVisible();
});

test("settings list the admin pages only for the admin", async ({ page, browser }) => {
  await signIn(page, await signUp("plain"));
  await page.goto("/settings");
  await expect(page.getByText("Moderation", { exact: true })).toBeVisible();
  await expect(page.getByText("Account standing")).toHaveCount(0);

  const adminPage = await pageFor(browser, await adminSession());
  await adminPage.goto("/settings");
  for (const label of ["Reports", "Account standing", "Account security", "Evidence"]) {
    await expect(adminPage.getByText(label, { exact: true })).toBeVisible();
  }
});

test("admin pages fit the screen", async ({ browser }) => {
  const adminPage = await pageFor(browser, await adminSession());
  for (const path of ADMIN_PAGES) {
    await adminPage.goto(path);
    // Not "networkidle": the notification stream keeps a request open.
    await expect(adminPage.getByText("Loading…")).toHaveCount(0);
    expect((await measure(adminPage)).horizontalScroll, `${path} scrolls sideways`).toBe(false);
  }
});
