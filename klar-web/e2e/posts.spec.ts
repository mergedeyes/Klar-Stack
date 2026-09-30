import { expect, test, type Page } from "@playwright/test";
import { apiCall, measure, signIn, signUp, uniqueName, upload } from "./helpers";

// Posts in the app: the modal gives an open post its own address, the
// share button hands out that address, and a shared link opens the
// full post page.

const modal = (page: Page) => page.locator("div.fixed.inset-0");
const path = (page: Page) => page.evaluate(() => location.pathname);

test.describe("post modal", () => {
  // The modal is opened from the feed on both screen sizes; the address
  // handling doesn't depend on the layout, so one project is enough.
  test.skip(({ isMobile }) => isMobile, "desktop only");

  test("opening gives the post its address; back, Escape and close restore it", async ({ page, request }) => {
    const alice = await signUp(request, "alice");
    const caption = uniqueName("caption");
    const post = await upload(request, alice, caption);
    await signIn(page, alice);
    await page.goto("/feed/discovery");
    const card = page.locator("article", { hasText: caption });

    for (const close of ["back", "escape", "button"] as const) {
      await card.getByLabel("View comments").click();
      await expect(modal(page)).toBeVisible();
      expect(await path(page)).toBe(`/posts/${post}`);
      if (close === "back") await page.goBack();
      if (close === "escape") await page.keyboard.press("Escape");
      if (close === "button") await page.getByLabel("Close").click();
      await expect(modal(page)).toHaveCount(0);
      expect(await path(page)).toBe("/feed/discovery");
      await expect(card).toBeVisible();
    }
  });

  test("links inside the modal leave its history entry", async ({ page, request }) => {
    const alice = await signUp(request, "alice");
    const caption = uniqueName("caption");
    await upload(request, alice, caption);
    await signIn(page, alice);
    await page.goto("/feed/discovery");

    await page.locator("article", { hasText: caption }).getByLabel("View comments").click();
    await modal(page).getByRole("link", { name: alice.username }).first().click();
    await expect(page).toHaveURL(new RegExp(`/users/${alice.username}$`));
    await page.goBack();
    await expect(page).toHaveURL(/\/feed\/discovery$/);
    await expect(modal(page)).toHaveCount(0);
  });

  test("share copies the post's link", async ({ page, request, context }) => {
    await context.grantPermissions(["clipboard-read", "clipboard-write"]);
    const alice = await signUp(request, "alice");
    const caption = uniqueName("caption");
    const post = await upload(request, alice, caption);
    await signIn(page, alice);
    await page.goto("/feed/discovery");

    await page.locator("article", { hasText: caption }).getByLabel("Share post").click();
    await expect(page.getByText("Link copied").first()).toBeVisible();
    expect(await page.evaluate(() => navigator.clipboard.readText())).toMatch(new RegExp(`/posts/${post}$`));
    await expect(modal(page)).toHaveCount(0);
  });

  test("reloading an open post shows the post page", async ({ page, request }) => {
    const alice = await signUp(request, "alice");
    const caption = uniqueName("caption");
    await upload(request, alice, caption);
    await signIn(page, alice);
    await page.goto("/feed/discovery");

    await page.locator("article", { hasText: caption }).getByLabel("View comments").click();
    await expect(modal(page)).toBeVisible();
    await page.reload();
    await expect(page.getByRole("heading", { name: "Post" }).or(page.getByText("Post", { exact: true })).first()).toBeVisible();
    await expect(page.locator("article")).toHaveCount(1);
    await expect(modal(page)).toHaveCount(0);
  });
});

test.describe("post page", () => {
  test("fits the layout to the screen size", async ({ page, request, isMobile }) => {
    const alice = await signUp(request, "alice");
    const post = await upload(request, alice, "A portrait photo", "portrait");
    await signIn(page, alice);
    await page.goto(`/posts/${post}`);
    await expect(page.getByText("A portrait photo").first()).toBeVisible();
    const input = page.getByLabel("Add a comment").locator("visible=true");
    const box = (await input.boundingBox())!;
    const m = await measure(page);

    // Always visible, just above the 40 px footer.
    expect(box.y + box.height).toBeLessThanOrEqual(m.screenHeight - 40 + 1);
    expect(m.horizontalScroll).toBe(false);
    if (isMobile) {
      expect(m.pageHeight, "phones scroll the whole page").toBeGreaterThan(m.screenHeight);
    } else {
      expect(m.pageHeight, "the card fits the screen").toBe(m.screenHeight);
    }
  });

  test("likes and comments work on the page", async ({ page, request }) => {
    const alice = await signUp(request, "alice");
    const post = await upload(request, alice, "Comment on me");
    await signIn(page, alice);
    await page.goto(`/posts/${post}`);

    await page.getByText("Be the first to like this").locator("visible=true").click();
    await expect(page.getByText("1 like").locator("visible=true")).toBeVisible();
    await page.getByLabel("Add a comment").locator("visible=true").fill("Written on the post page");
    await page.keyboard.press("Enter");
    await expect(page.getByText("Written on the post page")).toBeVisible();
  });

  test("an unavailable post gets a clear message", async ({ page }) => {
    await page.goto("/posts/00000000-0000-7000-8000-000000000000");
    await expect(page.getByText("This post isn't available")).toBeVisible();
  });

  test("deleting from the page returns to the profile", async ({ page, request }) => {
    const alice = await signUp(request, "alice");
    const post = await upload(request, alice, "Delete me");
    await signIn(page, alice);
    await page.goto(`/posts/${post}`);

    page.once("dialog", (dialog) => dialog.accept());
    await page.getByLabel("Delete post").locator("visible=true").click();
    await expect(page).toHaveURL(new RegExp(`/users/${alice.username}$`));
  });

  test("a private account's post needs a follow", async ({ page, request }) => {
    const bob = await signUp(request, "bob");
    const post = await upload(request, bob, "Private post");
    await apiCall(request, bob, "PATCH", "/users/me", { is_private: true });
    await page.goto(`/posts/${post}`);
    await expect(page.getByText("This post isn't available")).toBeVisible();
  });
});
