import { expect, test } from "@playwright/test";
import { apiCall, pageFor, signIn, signUp, uniqueName, upload } from "./helpers";

// A private account: following it sends a request, and its posts only open
// up once the request is accepted.

test("a private account's posts open up once the follow request is accepted", async ({ page, browser, request }) => {
  const [alice, bob] = await Promise.all([signUp("private"), signUp("follower")]);
  await apiCall(request, alice, "PATCH", "/users/me", { is_private: true });
  const caption = `private ${uniqueName("p")}`;
  const post = await upload(request, alice, caption);

  await signIn(page, bob);
  await page.goto(`/users/${alice.username}`);
  await expect(page.getByText("Follow this account to see their posts.")).toBeVisible();
  await page.getByRole("button", { name: "Follow", exact: true }).click();
  await expect(page.getByRole("button", { name: "Requested" })).toBeVisible();
  // Still closed while the request is pending.
  await page.goto(`/posts/${post}`);
  await expect(page.getByText(caption)).toBeHidden();

  const owner = await pageFor(browser, alice);
  await owner.goto("/follow-requests");
  const row = owner.locator("div").filter({ hasText: bob.username }).filter({ has: owner.getByRole("button", { name: "Accept" }) }).last();
  await row.getByRole("button", { name: "Accept" }).click();
  await expect(owner.getByRole("button", { name: "Accept" })).toHaveCount(0);

  await page.goto(`/users/${alice.username}`);
  await expect(page.getByRole("button", { name: "Following", exact: true })).toBeVisible();
  await expect(page.getByText("Follow this account to see their posts.")).toBeHidden();
  await page.goto(`/posts/${post}`);
  await expect(page.getByText(caption)).toBeVisible();
});

test("a declined request leaves the account closed and can be sent again", async ({ page, browser, request }) => {
  const [alice, bob] = await Promise.all([signUp("private"), signUp("follower")]);
  await apiCall(request, alice, "PATCH", "/users/me", { is_private: true });
  await apiCall(request, bob, "POST", `/users/${alice.username}/follow`);

  const owner = await pageFor(browser, alice);
  await owner.goto("/follow-requests");
  await owner.getByRole("button", { name: "Decline" }).click();
  await expect(owner.getByRole("button", { name: "Decline" })).toHaveCount(0);

  await signIn(page, bob);
  await page.goto(`/users/${alice.username}`);
  await expect(page.getByText("Follow this account to see their posts.")).toBeVisible();
  await expect(page.getByRole("button", { name: "Follow", exact: true })).toBeVisible();
});
