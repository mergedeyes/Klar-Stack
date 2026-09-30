import { defineConfig, devices } from "@playwright/test";

// Browser tests for behaviour only a real page shows: layout, the post
// modal's address and history, sharing, the post page, link-preview tags,
// and the moderation and admin flows.
// They run against a running backend and frontend (see e2e/README.md);
// CI starts both in the e2e job of .github/workflows/ci.yml.
export default defineConfig({
  testDir: "./e2e",
  // Registers and verifies the admin account (e2e/global-setup.ts).
  globalSetup: "./e2e/global-setup.ts",
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  workers: process.env.CI ? 2 : undefined,
  reporter: process.env.CI ? [["github"], ["html", { open: "never" }]] : "list",
  use: {
    baseURL: process.env.E2E_BASE_URL ?? "http://localhost:3001",
    trace: "retain-on-failure",
  },
  projects: [
    { name: "desktop", use: { ...devices["Desktop Chrome"], viewport: { width: 1280, height: 800 } } },
    // Touch and a coarse pointer, like a real phone.
    { name: "phone", use: { ...devices["Pixel 7"] } },
  ],
});
