import { expect, test, type Page } from "@playwright/test";
import { signIn, signUp, withDb } from "./helpers";
import { RELEASES } from "../src/whats-new/releases";

// The newest "What's new" release opens once over the feed for accounts
// older than it; /whats-new keeps all of them.

const latest = RELEASES[0];
test.skip(!latest, "no release yet");

/** Opens the feed and waits for GET /users/me, which decides the dialog. */
async function openFeed(page: Page, reload = false) {
  const me = page.waitForResponse((res) => res.url().endsWith("/users/me") && res.request().method() === "GET");
  await (reload ? page.reload() : page.goto("/feed"));
  await me;
}

test("an older account sees the newest release once", async ({ page }) => {
  const alice = await signUp("alice", undefined, { whatsNew: true });
  await withDb((db) => db.query("UPDATE users SET created_at = '2026-01-01T12:00:00Z' WHERE id = $1", [alice.id]));
  await signIn(page, alice);
  await openFeed(page);

  const dialog = page.getByRole("dialog", { name: "What's new" });
  await expect(dialog).toBeVisible();
  await expect(dialog.getByText(latest.en.items[0].title)).toBeVisible();
  await dialog.getByRole("button", { name: "Got it" }).click();
  await expect(dialog).toBeHidden();

  await openFeed(page, true);
  await expect(dialog).toBeHidden();

  // Still there to read, from the footer.
  await page.getByRole("link", { name: "What's new" }).click();
  await expect(page).toHaveURL(/\/whats-new$/);
  await expect(page.getByText(latest.en.items[0].title)).toBeVisible();
});

test("an account newer than the release doesn't see it", async ({ page }) => {
  const bob = await signUp("bob", undefined, { whatsNew: true });
  // Signed up on the release's own day: not before it.
  await withDb((db) => db.query("UPDATE users SET created_at = $2 WHERE id = $1", [bob.id, `${latest.id.slice(0, 10)}T12:00:00Z`]));
  await signIn(page, bob);
  await openFeed(page);
  await expect(page.getByRole("dialog", { name: "What's new" })).toBeHidden();
});
