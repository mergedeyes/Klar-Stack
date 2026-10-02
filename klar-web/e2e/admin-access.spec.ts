import { expect, test } from "@playwright/test";
import { adminSession, measure, pageFor, signIn, signUp } from "./helpers";

// Admin pages: closed to everyone else, reached from the admin's header
// button, and usable at phone width.

const ADMIN_PAGES = ["/admin", "/admin/audit", "/admin/reports", "/admin/standing", "/admin/security", "/admin/moderation", "/admin/evidence"];

test("the admin pages refuse ordinary accounts", async ({ page }) => {
  await signIn(page, await signUp("nosy"));
  for (const path of ["/admin", "/admin/audit", "/admin/standing", "/admin/security", "/admin/reports"]) {
    await page.goto(path);
    await expect(page.getByText("Admin access required")).toBeVisible();
  }
  await page.goto("/admin/review/someone");
  await page.getByLabel("Reason").fill("curious");
  await page.getByRole("button", { name: "Open review" }).click();
  await expect(page.getByText("Admin access required")).toBeVisible();
});

test("only the admin gets the moderation tools button, which lists the admin pages by category", async ({ page, browser }) => {
  await signIn(page, await signUp("plain"));
  await page.goto("/feed");
  await expect(page.getByRole("button", { name: "Settings" })).toBeVisible();
  await expect(page.getByRole("button", { name: /Moderation tools/ })).toHaveCount(0);
  // The user's own moderation page stays in settings; the admin pages don't.
  await page.goto("/settings");
  await expect(page.getByText("Moderation", { exact: true })).toBeVisible();
  await expect(page.getByText("Account standing")).toHaveCount(0);

  const adminPage = await pageFor(browser, await adminSession());
  await adminPage.goto("/feed");
  await adminPage.getByRole("button", { name: /Moderation tools/ }).click();
  await expect(adminPage).toHaveURL(/\/admin$/);
  const categories = {
    Moderation: ["Reports", "Statements & objections", "Rights claims", "Decision log"],
    Accounts: ["Account standing", "Account security", "Official accounts"],
    Legal: ["Evidence", "Legal updates", "Audit export"],
    Testing: ["Feedback"],
  };
  for (const [title, labels] of Object.entries(categories)) {
    const section = adminPage.getByRole("region", { name: title, exact: true });
    for (const label of labels) {
      await expect(section.getByRole("link", { name: new RegExp(`^${label}`) })).toBeVisible();
    }
  }
  await adminPage.getByRole("region", { name: "Accounts", exact: true }).getByRole("link", { name: /^Account standing/ }).click();
  await expect(adminPage).toHaveURL(/\/admin\/standing$/);

  await adminPage.goto("/settings");
  await expect(adminPage.getByText("Account standing")).toHaveCount(0);
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
