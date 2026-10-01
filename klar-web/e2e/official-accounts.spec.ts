import { expect, test } from "@playwright/test";
import { adminSession, pageFor, signUp, uniqueName, withDb } from "./helpers";

// Official accounts (a verified @klarsocial.eu address) get staff names that
// the sign-up form refuses, through the admin page; every rename is logged.

test("an admin gives a verified @klarsocial.eu account a staff name", async ({ browser }, testInfo) => {
  const admin = await adminSession();
  // Staff names are unique across the database: each project takes its own
  // and frees it first (a rerun on the same test database had it already).
  const staffName = testInfo.project.name === "phone" ? "Support" : "Help";
  await withDb((db) =>
    db.query("UPDATE users SET username = 'freed_' || substr(md5(random()::text), 1, 12) WHERE LOWER(username) = LOWER($1)", [staffName]),
  );
  const email = `${uniqueName("team")}@klarsocial.eu`;
  const official = await signUp("team", email);

  const adminPage = await pageFor(browser, admin);
  await adminPage.goto("/admin/official");
  // Unverified: not official yet.
  await expect(adminPage.getByText(email)).toHaveCount(0);
  await withDb((db) => db.query("UPDATE users SET email_verified = TRUE WHERE id = $1", [official.id]));
  await adminPage.reload();

  const card = adminPage.locator("form").filter({ hasText: email });
  await card.getByPlaceholder("New username").fill(staffName);
  const rename = card.getByRole("button", { name: "Rename" });
  await expect(rename).toBeDisabled();
  await card.getByPlaceholder("Why? (required, kept in the log)").fill("The team's support profile");
  await rename.click();
  await expect(adminPage.getByRole("status")).toHaveText(`@${official.username} is now @${staffName}.`);
  const log = adminPage.locator("div.rounded-xl").filter({ hasText: `@${official.username} → @${staffName}` });
  await expect(log).toContainText("The team's support profile");
  await expect(log).toContainText("by @e2e_admin");

  const visitor = await pageFor(browser, await signUp("visitor"));
  await visitor.goto(`/users/${staffName.toLowerCase()}`);
  await expect(visitor.getByText(staffName, { exact: true }).first()).toBeVisible();
});
