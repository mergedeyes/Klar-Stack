import fs from "node:fs";
import { expect, request as playwrightRequest, test } from "@playwright/test";
import { API, clientIp, pageFor, signIn, signUp, uniqueName, withDb } from "./helpers";

// The account's life cycle in the browser: signing up, verifying the email
// through the link, a forgotten password, changing it (which signs out the
// other devices), downloading the data and deleting the account. Links that
// would arrive by email are read from the test database.

async function emailToken(userEmail: string, type: "verification" | "password_reset"): Promise<string> {
  return withDb(async (db) => {
    const res = await db.query(
      `SELECT t.token FROM email_tokens t JOIN users u ON u.id = t.user_id
       WHERE LOWER(u.email) = LOWER($1) AND t.token_type = $2 AND t.used_at IS NULL
       ORDER BY t.created_at DESC LIMIT 1`,
      [userEmail, type],
    );
    expect(res.rowCount, `a ${type} token for ${userEmail}`).toBe(1);
    return res.rows[0].token as string;
  });
}

async function refreshStatus(refreshToken: string): Promise<number> {
  const context = await playwrightRequest.newContext();
  try {
    const res = await context.post(`${API}/auth/refresh`, {
      headers: { "X-Forwarded-For": clientIp() },
      data: { refresh_token: refreshToken },
    });
    return res.status();
  } finally {
    await context.dispose();
  }
}

test("signing up in the form, then verifying the email through the link", async ({ browser, baseURL }) => {
  test.skip(!process.env.E2E_DATABASE_URL, "reads the emailed link from E2E_DATABASE_URL (see e2e/README.md)");
  const context = await browser.newContext({ baseURL });
  const page = await context.newPage();
  const username = uniqueName("new");
  const email = `${username}@example.test`;

  await page.goto("/register");
  await page.getByPlaceholder("Username").fill(username);
  await page.getByPlaceholder("you@example.com").fill(email);
  await page.getByLabel("Password", { exact: true }).fill("a-good-password-1");
  await page.getByLabel("Confirm password").fill("a-good-password-1");
  await page.getByRole("checkbox").check();
  await page.getByRole("button", { name: "Create account" }).click();
  await expect(page.getByText(`We sent a verification link to`)).toBeVisible();
  await page.getByRole("button", { name: "Continue to Klar" }).click();
  await expect(page).toHaveURL(/\/feed/);
  await expect(page.getByText("Please verify your email address.")).toBeVisible();

  await page.goto(`/verify-email?token=${await emailToken(email, "verification")}`);
  await expect(page.getByText("Email verified!")).toBeVisible();
  await page.goto("/feed");
  await expect(page.getByText("Please verify your email address.")).toBeHidden();
  await context.close();
});

test("a forgotten password is reset through the link, and the new one signs in", async ({ browser, baseURL }) => {
  test.skip(!process.env.E2E_DATABASE_URL, "reads the emailed link from E2E_DATABASE_URL (see e2e/README.md)");
  const user = await signUp("forgetful");
  const email = `${user.username}@example.test`;
  const context = await browser.newContext({ baseURL });
  const page = await context.newPage();

  await page.goto("/forgot-password");
  await page.getByPlaceholder("you@example.com").fill(email);
  await page.getByRole("button", { name: "Send reset link" }).click();
  await expect(page.getByText("Check your email")).toBeVisible();

  await page.goto(`/reset-password?token=${await emailToken(email, "password_reset")}`);
  await page.getByLabel("New password", { exact: true }).fill("brand-new-password-7");
  await page.getByLabel("Confirm new password").fill("brand-new-password-7");
  await page.getByRole("button", { name: "Reset password" }).click();
  await expect(page.getByText("Password reset!")).toBeVisible();
  await expect(page).toHaveURL(/\/login/);

  // The old session is over; the new password works.
  expect(await refreshStatus(user.refresh_token)).toBe(401);
  await page.getByPlaceholder("you@example.com").fill(email);
  await page.getByLabel("Password").fill("brand-new-password-7");
  await page.getByRole("button", { name: "Sign in" }).click();
  await expect(page).toHaveURL(/\/feed/);
  await context.close();
});

test("changing the password signs out the other devices", async ({ page }) => {
  const user = await signUp("changer");
  // A second device: its own login, its own refresh token.
  const loginContext = await playwrightRequest.newContext();
  const login = await loginContext.post(`${API}/auth/login`, {
    headers: { "X-Forwarded-For": clientIp() },
    data: { email: `${user.username}@example.test`, password: "test-password-123" },
  });
  const secondDevice = (await login.json()).refresh_token as string;
  await loginContext.dispose();

  await signIn(page, user);
  await page.goto("/settings/password");
  await page.getByLabel("Current password").fill("test-password-123");
  await page.getByLabel("New password", { exact: true }).fill("changed-password-9");
  await page.getByLabel("Confirm new password").fill("changed-password-9");
  await page.getByRole("button", { name: "Change password" }).click();
  await expect(page).toHaveURL(/\/settings$/);

  expect(await refreshStatus(secondDevice)).toBe(401);
});

test("the data download is a ZIP, and deleting the account signs out and removes the profile", async ({ page, browser }) => {
  const user = await signUp("leaving");
  await signIn(page, user);

  await page.goto("/settings");
  const download = page.waitForEvent("download");
  await page.getByRole("button", { name: /Download your data/ }).click();
  const file = await download;
  expect(file.suggestedFilename()).toMatch(/^klar-datenexport.*\.zip$/);
  // A real ZIP archive ("PK" signature), not an error page.
  const bytes = fs.readFileSync((await file.path())!);
  expect(bytes.subarray(0, 2).toString()).toBe("PK");

  await page.goto("/settings/account");
  await page.getByRole("button", { name: /Permanently delete your account and all data/ }).click();
  const confirm = page.getByRole("button", { name: "Delete account" });
  await expect(confirm).toBeDisabled();
  await page.getByPlaceholder(user.username).fill(user.username);
  await confirm.click();
  await expect(page).toHaveURL(/\/login/);

  // For anyone else the profile is gone (the page falls back to the feed).
  const visitor = await pageFor(browser, await signUp("visitor"));
  await visitor.goto(`/users/${user.username}`);
  await expect(visitor).toHaveURL(/\/feed/);
});
