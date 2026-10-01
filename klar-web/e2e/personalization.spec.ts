import { expect, test } from "@playwright/test";
import { signIn, signUp } from "./helpers";

// Personalised Discovery is opt-in: off for a new account, switched on
// with one click, and switching it off asks first (it deletes the stored
// interactions). Both are saved.
test("personalised Discovery is off until switched on, and switching it off asks first", async ({ page }) => {
  await signIn(page, await signUp("chooser"));
  await page.goto("/settings");

  const toggle = page.getByRole("switch", { name: "Personalised Discovery" });
  await expect(toggle).toBeEnabled();
  await expect(toggle).toHaveAttribute("aria-checked", "false");

  await toggle.click();
  await expect(toggle).toHaveAttribute("aria-checked", "true");
  await page.reload();
  await expect(toggle).toHaveAttribute("aria-checked", "true");

  // Cancelling the confirmation leaves it on.
  page.once("dialog", (dialog) => dialog.dismiss());
  await toggle.click();
  await expect(toggle).toHaveAttribute("aria-checked", "true");

  page.once("dialog", (dialog) => dialog.accept());
  await toggle.click();
  await expect(toggle).toHaveAttribute("aria-checked", "false");
  await page.reload();
  await expect(toggle).toHaveAttribute("aria-checked", "false");
});
