import { expect, test } from "@playwright/test";
import { apiCall, comment, signIn, signUp, upload, uniqueName } from "./helpers";

// The settings overview's own switch, and the two pages behind it that list
// the account's blocks and its likes and comments.

test("the private account switch saves at once and asks before going public", async ({ page }) => {
  await signIn(page, await signUp("private"));
  await page.goto("/settings");
  const toggle = page.getByRole("switch", { name: "Private account" });
  await expect(toggle).toHaveAttribute("aria-checked", "false");
  await toggle.click();
  await expect(toggle).toHaveAttribute("aria-checked", "true");
  await page.reload();
  await expect(toggle).toHaveAttribute("aria-checked", "true");

  // Cancelling the confirmation leaves the account private.
  page.once("dialog", (dialog) => dialog.dismiss());
  await toggle.click();
  await expect(toggle).toHaveAttribute("aria-checked", "true");
  page.once("dialog", (dialog) => dialog.accept());
  await toggle.click();
  await expect(toggle).toHaveAttribute("aria-checked", "false");
});

test("blocked accounts are listed and can be unblocked there", async ({ page, request }) => {
  const [alice, bob] = await Promise.all([signUp("blocker"), signUp("blocked")]);
  await apiCall(request, alice, "POST", `/users/${bob.username}/block`);

  await signIn(page, alice);
  await page.goto("/settings");
  await page.getByRole("link", { name: "Blocked accounts" }).click();
  await expect(page).toHaveURL(/\/settings\/blocked$/);
  await expect(page.getByText(bob.username, { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "Unblock" }).click();
  // Stays in the list, so a mistaken click can be undone.
  await expect(page.getByRole("button", { name: "Block", exact: true })).toBeVisible();
  await page.reload();
  await expect(page.getByText("You haven't blocked anyone.")).toBeVisible();
});

test("your activity lists liked posts and own comments, which can be deleted there", async ({ page, request }) => {
  const [alice, bob] = await Promise.all([signUp("active"), signUp("poster")]);
  const post = await upload(request, bob, `liked ${uniqueName("p")}`);
  await apiCall(request, alice, "POST", `/posts/${post}/like`);
  const text = `my comment ${uniqueName("c")}`;
  await comment(request, alice, post, text);

  await signIn(page, alice);
  await page.goto("/settings");
  await page.getByRole("link", { name: "Your activity" }).click();
  await expect(page.getByRole("button", { name: `Post by ${bob.username}` })).toBeVisible();

  await page.getByRole("tab", { name: "Comments" }).click();
  await expect(page.getByText(text)).toBeVisible();
  await expect(page.getByText(`On ${bob.username}'s post`)).toBeVisible();
  page.once("dialog", (dialog) => dialog.accept());
  await page.getByRole("button", { name: "Delete comment" }).click();
  await expect(page.getByText(text)).toBeHidden();
  await page.reload();
  await page.getByRole("tab", { name: "Comments" }).click();
  await expect(page.getByText("Comments you write show up here.")).toBeVisible();
});
