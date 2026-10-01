import { expect, test } from "@playwright/test";
import { apiCall, signIn, signUp, uniqueName, upload } from "./helpers";

// The profile grid loads 30 posts at a time and the next page when its end
// scrolls into view, until the real end. (The cursor itself, with posts from
// the same instant, is tested in the backend: integration_tests/social.rs.)

test.skip(({ isMobile }) => isMobile, "same paging logic on both; one screen size is enough");

test("the profile grid keeps loading as you scroll and stops at the end", async ({ page, request }) => {
  const [author, viewer] = await Promise.all([signUp("prolific"), signUp("viewer")]);
  // Private, so these posts stay out of the discovery feed that other tests
  // running at the same time look for their own posts in.
  await apiCall(request, author, "PATCH", "/users/me", { is_private: true });
  await apiCall(request, viewer, "POST", `/users/${author.username}/follow`);
  await apiCall(request, author, "POST", `/users/me/follow-requests/${viewer.username}/accept`);
  const prefix = uniqueName("grid");
  const total = 34;
  for (let start = 0; start < total; start += 6) {
    await Promise.all(
      Array.from({ length: Math.min(6, total - start) }, (_, i) => upload(request, author, `${prefix} ${start + i}`)),
    );
  }

  await signIn(page, viewer);
  await page.goto(`/users/${author.username}`);
  const cells = page.getByRole("img", { name: new RegExp(`^${prefix} \\d+$`) });
  await expect(cells).toHaveCount(30);

  await cells.last().scrollIntoViewIfNeeded();
  await page.mouse.wheel(0, 5000);
  await expect(cells).toHaveCount(total);

  // Every post once, none twice.
  const names = await cells.evaluateAll((imgs) => imgs.map((img) => img.getAttribute("alt")));
  expect(new Set(names).size).toBe(total);

  // At the end nothing more is requested.
  let extra = 0;
  page.on("request", (req) => {
    if (req.url().includes(`/users/${author.username}/posts`)) extra++;
  });
  await page.mouse.wheel(0, 5000);
  await page.waitForTimeout(500);
  expect(extra).toBe(0);
});
