import { expect, test, type Page } from "@playwright/test";
import { signIn, signUp } from "./helpers";

// The welcome guide (components/WelcomeGuide.tsx): once over the feed for a
// new account, and from the footer link whenever someone wants it again.

/** A button in the guide; exact, since dev mode adds a "Next.js" button. */
function guideButton(page: Page, name: string) {
  return page.getByRole("dialog").getByRole("button", { name, exact: true });
}

/** Waits for closing the guide to be saved, so a reload doesn't cancel it. */
function guideSaved(page: Page) {
  return page.waitForResponse((r) => r.url().endsWith("/users/me/guide-seen") && r.ok());
}

test("a new account gets the guide once, and the footer opens it again", async ({ page }) => {
  await signIn(page, await signUp("newcomer", undefined, { guide: true }));
  await page.goto("/feed");

  const guide = page.getByRole("dialog", { name: "Welcome to Klar" });
  await expect(guide).toBeVisible();
  for (const title of ["Find people", "Share and react", "Chats", "Staying safe"]) {
    await guideButton(page, "Next").click();
    await expect(page.getByRole("dialog", { name: title })).toBeVisible();
  }
  await guideButton(page, "Next").click();
  const choices = page.getByRole("dialog", { name: "Your choices" });
  await expect(choices.getByRole("checkbox", { name: /Keep my account after the test/ })).not.toBeChecked();
  const saved = guideSaved(page);
  await guideButton(page, "Let's go").click();
  await expect(page.getByRole("dialog")).toBeHidden();
  await saved;

  // Seen is stored for the account, not just this tab.
  await page.reload();
  await expect(page.getByText("Your feed is empty")).toBeVisible();
  await expect(page.getByRole("dialog")).toBeHidden();

  await page.locator("footer").getByRole("button", { name: "Guide" }).click();
  await expect(page.getByRole("dialog", { name: "Welcome to Klar" })).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog")).toBeHidden();
});

test("the keep-account box in the guide saves the same setting as in Settings", async ({ page }) => {
  await signIn(page, await signUp("keeper", undefined, { guide: true }));
  await page.goto("/feed");
  for (let i = 0; i < 5; i++) await guideButton(page, "Next").click();
  await page.getByRole("checkbox", { name: /Keep my account after the test/ }).check();
  await expect(page.getByText(/^Requested on/)).toBeVisible();
  await guideButton(page, "Let's go").click();

  await page.goto("/settings");
  await expect(page.getByRole("checkbox", { name: /Keep my account after the test/ })).toBeChecked();
});

test("skipping the guide counts as seen", async ({ page }) => {
  await signIn(page, await signUp("skipper", undefined, { guide: true }));
  await page.goto("/feed");
  const saved = guideSaved(page);
  await guideButton(page, "Skip").click();
  await expect(page.getByRole("dialog")).toBeHidden();
  await saved;
  await page.reload();
  await expect(page.getByText("Your feed is empty")).toBeVisible();
  await expect(page.getByRole("dialog")).toBeHidden();
});
