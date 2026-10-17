import { expect, test } from "@playwright/test";

// Klar can be added to the home screen as an app (app/manifest.ts): Chrome
// itself finds nothing missing, and it opens on the feed.

test.skip(({ isMobile }) => isMobile, "one browser is enough");

test("Chrome can install Klar as an app", async ({ page, request }) => {
  await page.goto("/login");
  const cdp = await page.context().newCDPSession(page);
  // Chrome needs the manifest and its icons fetched before it can tell.
  await expect
    .poll(async () => (await cdp.send("Page.getAppManifest")).url ?? "")
    .toContain("/manifest.webmanifest");
  const { installabilityErrors } = await cdp.send("Page.getInstallabilityErrors");
  expect(installabilityErrors).toEqual([]);

  const manifest = await (await request.get("/manifest.webmanifest")).json();
  expect(manifest.start_url).toBe("/feed");
  expect(manifest.display).toBe("standalone");
});
