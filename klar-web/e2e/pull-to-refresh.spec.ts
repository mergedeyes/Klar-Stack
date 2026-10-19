import { expect, test, type Page } from "@playwright/test";
import { apiCall, asHomeScreenApp, signIn, signUp, upload } from "./helpers";

// Pulling down at the top reloads the page, but only in the home screen
// app; a browser tab keeps its own pull-to-refresh.

test.skip(({ isMobile }) => !isMobile, "a touch gesture");

/** A real touch drag from (200, y) down by `distance` pixels. */
async function pullDown(page: Page, distance: number) {
  const cdp = await page.context().newCDPSession(page);
  const y = 200;
  await cdp.send("Input.dispatchTouchEvent", { type: "touchStart", touchPoints: [{ x: 200, y }] });
  for (let step = 1; step <= 10; step++) {
    await cdp.send("Input.dispatchTouchEvent", { type: "touchMove", touchPoints: [{ x: 200, y: y + (distance * step) / 10 }] });
  }
  await cdp.send("Input.dispatchTouchEvent", { type: "touchEnd", touchPoints: [] });
}

async function feedWithANewPost(page: Page, request: Parameters<typeof upload>[0]) {
  const [alice, bob] = await Promise.all([signUp("alice"), signUp("bob")]);
  await apiCall(request, alice, "POST", `/users/${bob.username}/follow`);
  await upload(request, bob, "the older post");
  await signIn(page, alice);
  await page.goto("/feed");
  await expect(page.getByText("the older post")).toBeVisible();
  await upload(request, bob, "posted after opening");
}

test("in the home screen app, pulling down at the top reloads the feed", async ({ page, request }) => {
  await asHomeScreenApp(page);
  await feedWithANewPost(page, request);
  await pullDown(page, 300);
  await expect(page.getByText("posted after opening")).toBeVisible();
});

test("in a browser tab the page leaves pulling to the browser", async ({ page, request }) => {
  await feedWithANewPost(page, request);
  await pullDown(page, 300);
  await expect(page.getByTestId("pull-to-refresh")).toHaveCount(0);
  await expect(page.getByText("posted after opening")).toBeHidden();
});
