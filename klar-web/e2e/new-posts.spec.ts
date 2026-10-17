import { expect, test } from "@playwright/test";
import { apiCall, signIn, signUp, upload } from "./helpers";

// New posts don't push into an open feed on their own: a pill counts them,
// and tapping it shows them.

test("posts that come in while the feed is open wait behind a pill", async ({ page, request }) => {
  const [alice, bob] = await Promise.all([signUp("alice"), signUp("bob")]);
  await apiCall(request, alice, "POST", `/users/${bob.username}/follow`);
  await upload(request, bob, "the older post");

  await signIn(page, alice);
  const stream = page.waitForResponse((res) => res.url().includes("/notifications/stream?"));
  await page.goto("/feed");
  await stream;
  await expect(page.getByText("the older post")).toBeVisible();

  await upload(request, bob, "a newer post");
  await upload(request, bob, "the newest post");
  // Coming back to the app is one of the moments the feed looks for more.
  await page.evaluate(() => document.dispatchEvent(new Event("visibilitychange")));

  const pill = page.getByRole("button", { name: "2 new posts" });
  await expect(pill).toBeVisible();
  await expect(page.getByText("the newest post")).toBeHidden();

  await pill.click();
  await expect(page.getByText("the newest post")).toBeVisible();
  await expect(page.getByText("a newer post")).toBeVisible();
  await expect(pill).toBeHidden();
});
