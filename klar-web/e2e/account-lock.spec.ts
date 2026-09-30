import { expect, test } from "@playwright/test";
import { adminSession, API, pageFor, signIn, signUp, withDb } from "./helpers";

// Locking an account that looks taken over: signed out everywhere, the
// login explains the lock, a new password unlocks it, and the incident log
// keeps the record.

const PASSWORD = "test-password-123";

async function logIn(page: import("@playwright/test").Page, username: string, password: string) {
  await page.goto("/login");
  await page.getByLabel("Email").fill(`${username}@example.test`);
  await page.getByLabel("Password").fill(password);
  await page.getByRole("button", { name: "Sign in" }).click();
}

test("a locked account is signed out, the login explains the lock, and a new password unlocks it", async ({ page, browser, request }) => {
  const admin = await adminSession();
  const victim = await signUp("victim");

  // Signed in and browsing.
  await signIn(page, victim);
  await page.goto("/moderation");
  await expect(page.getByText("Account status")).toBeVisible();

  // The admin locks it from the security page.
  const adminPage = await pageFor(browser, admin);
  await adminPage.goto("/admin/security");
  await adminPage.getByLabel("Username", { exact: true }).fill(victim.username);
  await adminPage.getByLabel("Reason").fill("Posted 40 crypto links in five minutes");
  await adminPage.getByRole("button", { name: "Lock and email the owner" }).click();
  await expect(adminPage.getByText(`@${victim.username} is locked and has been emailed.`)).toBeVisible();

  // The victim's session ends at the next request.
  const res = await request.get(`${API}/users/me`, { headers: { Authorization: `Bearer ${victim.access_token}` } });
  expect(res.status()).toBe(401);
  await page.goto("/moderation");
  await expect(page).toHaveURL(/\/login/);

  // A wrong password looks like any wrong password...
  await logIn(page, victim.username, "wrong-password-9");
  await expect(page.getByText("Invalid email or password")).toBeVisible();
  await expect(page.getByText("Your account is locked")).toBeHidden();

  // ...the right one explains the lock; a new link only every 15 minutes.
  await logIn(page, victim.username, PASSWORD);
  await expect(page.getByText("Your account is locked")).toBeVisible();
  await expect(page.getByRole("link", { name: "kontakt@klarsocial.eu" })).toBeVisible();
  await page.getByRole("button", { name: "Send the link again" }).click();
  await expect(page.getByRole("status")).toContainText("every 15 minutes");

  // The emailed link (read from the test database, the email goes nowhere).
  const token = await withDb(async (db) => {
    const r = await db.query(
      "SELECT token FROM email_tokens WHERE user_id = $1 AND token_type = 'password_reset' ORDER BY expires_at DESC LIMIT 1",
      [victim.id],
    );
    return r.rows[0].token as string;
  });
  await page.goto(`/reset-password?token=${token}`);
  await page.getByLabel("New password", { exact: true }).fill("a-new-password-42");
  await page.getByLabel("Confirm new password").fill("a-new-password-42");
  await page.getByRole("button", { name: /reset password/i }).click();
  await expect(page.getByText("Password reset!")).toBeVisible();

  await logIn(page, victim.username, "a-new-password-42");
  await expect(page).toHaveURL(/\/feed/);

  // The incident log shows how it ended and keeps an assessment.
  await adminPage.reload();
  const entry = adminPage.locator("div.rounded-xl").filter({ hasText: `@${victim.username}` }).filter({ hasText: "Posted 40 crypto links" });
  await expect(entry.getByText("Unlocked by new password")).toBeVisible();
  await entry.getByLabel(/Assessment/).fill("Only public posts affected; low risk, not reported.");
  await entry.getByRole("button", { name: "Save assessment" }).click();
  await adminPage.reload();
  await expect(entry.getByLabel(/Assessment/)).toHaveValue("Only public posts affected; low risk, not reported.");
});

test("an admin can unlock without a new password", async ({ page, browser }) => {
  const admin = await adminSession();
  const owner = await signUp("owner");
  const adminPage = await pageFor(browser, admin);
  await adminPage.goto("/admin/security");
  await adminPage.getByLabel("Username", { exact: true }).fill(owner.username);
  await adminPage.getByLabel("Reason").fill("Odd login pattern reported by a friend");
  await adminPage.getByRole("button", { name: "Lock and email the owner" }).click();

  const entry = adminPage.locator("div.rounded-xl").filter({ hasText: "Odd login pattern reported by a friend" }).filter({ hasText: owner.username });
  await expect(entry.getByText("Locked", { exact: true })).toBeVisible();
  await entry.getByRole("button", { name: "Unlock" }).click();
  await expect(entry.getByText("Unlocked by admin")).toBeVisible();

  await logIn(page, owner.username, PASSWORD);
  await expect(page).toHaveURL(/\/feed/);
});
