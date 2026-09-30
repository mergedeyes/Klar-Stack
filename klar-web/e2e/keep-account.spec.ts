import { expect, test } from "@playwright/test";
import { signIn, signUp } from "./helpers";

// Test phase: the opt-in to keep an account through the pre-launch wipe
// is off by default and survives a reload once ticked.
test("keeping the account is opt-in and saved", async ({ page }) => {
  await signIn(page, await signUp("tester"));
  await page.goto("/settings");

  const keep = page.getByRole("checkbox", { name: /Keep my account after the test/ });
  await expect(keep).toBeEnabled();
  await expect(keep).not.toBeChecked();
  await keep.check();
  await expect(page.getByText(/Requested on/)).toBeVisible();

  await page.reload();
  await expect(keep).toBeChecked();
  await keep.uncheck();
  await expect(page.getByText(/Requested on/)).toBeHidden();
});
