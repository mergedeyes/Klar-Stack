import { expect, test } from "@playwright/test";
import { adminSession, pageFor } from "./helpers";

// The audit export needs a reason, downloads a ZIP named after the period,
// and is listed with its reason afterwards.
test("an audit export needs a reason, downloads, and is logged", async ({ browser }) => {
  const adminPage = await pageFor(browser, await adminSession());
  await adminPage.goto("/admin/audit");

  const button = adminPage.getByRole("button", { name: "Download export" });
  await expect(button).toBeDisabled();
  const reason = `E2E request ${Date.now()}`;
  await adminPage.getByLabel("Reason (who asked, and what for)").fill(reason);
  await expect(button).toBeEnabled();

  const download = adminPage.waitForEvent("download");
  await button.click();
  expect((await download).suggestedFilename()).toMatch(/^klar-audit-\d{4}-\d{2}-\d{2}_\d{4}-\d{2}-\d{2}\.zip$/);

  const logged = adminPage.getByRole("listitem").filter({ hasText: reason });
  await expect(logged).toBeVisible();
  await expect(logged.getByText("with identities")).toHaveCount(0);
  await expect(adminPage.getByLabel("Reason (who asked, and what for)")).toHaveValue("");
});
