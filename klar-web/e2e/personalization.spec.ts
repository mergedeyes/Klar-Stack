import { expect, test } from "@playwright/test";
import { signIn, signUp } from "./helpers";

// The opt-out for personalised Discovery: on by default, switching it off
// asks first (it deletes the stored interactions) and is saved.
test("personalised Discovery can be switched off, after a confirmation", async ({ page }) => {
  await signIn(page, await signUp("chooser"));
  await page.goto("/settings");

  const toggle = page.getByRole("switch", { name: "Personalised Discovery" });
  await expect(toggle).toBeEnabled();
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
  // Switching it back on needs no confirmation.
  await toggle.click();
  await expect(toggle).toHaveAttribute("aria-checked", "true");
});
