import { expect, test, type Page } from "@playwright/test";
import { signIn, signUp } from "./helpers";

// Personalised Discovery is opt-in: off for a new account, switched on
// with one click, and switching it off asks first (it deletes the stored
// interactions). Both are saved.

// The switch flips at once; reloading before the save is answered would
// read the old value back.
function saved(page: Page) {
  return page.waitForResponse((r) => r.request().method() === "PATCH" && r.url().includes("/users/me/personalization"));
}

test("personalised Discovery is off until switched on, and switching it off asks first", async ({ page }) => {
  await signIn(page, await signUp("chooser"));
  await page.goto("/settings");
  await page.getByText("Personalised Discovery").click();
  await expect(page).toHaveURL(/\/settings\/discovery$/);

  const toggle = page.getByRole("switch", { name: "Personalised Discovery" });
  await expect(toggle).toBeEnabled();
  await expect(toggle).toHaveAttribute("aria-checked", "false");

  let save = saved(page);
  await toggle.click();
  await save;
  await expect(toggle).toHaveAttribute("aria-checked", "true");
  await page.reload();
  await expect(toggle).toHaveAttribute("aria-checked", "true");

  // Cancelling the confirmation leaves it on.
  page.once("dialog", (dialog) => dialog.dismiss());
  await toggle.click();
  await expect(toggle).toHaveAttribute("aria-checked", "true");

  page.once("dialog", (dialog) => dialog.accept());
  save = saved(page);
  await toggle.click();
  await save;
  await expect(toggle).toHaveAttribute("aria-checked", "false");
  await page.reload();
  await expect(toggle).toHaveAttribute("aria-checked", "false");
});
