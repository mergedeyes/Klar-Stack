import { expect, test } from "@playwright/test";
import { ADMIN_USERNAME, adminSession, apiCall, pageFor, report, signUp, upload } from "./helpers";

// "Show closed" on the report queue lists what was decided, including
// dismissals, which leave no decision behind. The queue is shared between
// tests, so the report is found by its own unique note.
test("closed reports show the outcome, who closed them and the note", async ({ browser, request }) => {
  const admin = await adminSession();
  const [author, reporter] = [await signUp("closedauthor"), await signUp("closedreporter")];
  const post = await upload(request, author, "Harmless holiday photo");
  const reportId = await report(request, reporter, "post", post, "spam");
  const note = `Not spam ${Date.now()}`;
  await apiCall(request, admin, "POST", `/admin/reports/${reportId}/dismiss`, { note });

  const adminPage = await pageFor(browser, admin);
  await adminPage.goto("/admin/reports");
  await adminPage.getByLabel("Show closed").check();
  const closed = adminPage.getByTestId("closed-report").filter({ hasText: note });
  await expect(closed).toBeVisible();
  await expect(closed).toContainText("Reviewed — no violation found");
  await expect(closed).toContainText(`@${author.username}`);
  await expect(closed).toContainText(`reported by @${reporter.username}`);
  await expect(closed).toContainText(`closed by @${ADMIN_USERNAME}`);

  await adminPage.getByLabel("Outcome").selectOption("removed");
  await expect(adminPage.getByTestId("closed-report").filter({ hasText: note })).toHaveCount(0);

  // Back to the queue.
  await adminPage.getByLabel("Show closed").uncheck();
  await expect(adminPage.getByRole("region", { name: "Closed reports" })).toHaveCount(0);
});

test("the decision log points to dismissed reports", async ({ browser, request }) => {
  const admin = await adminSession();
  const [author, reporter] = [await signUp("logauthor"), await signUp("logreporter")];
  const post = await upload(request, author, "Another harmless photo");
  const reportId = await report(request, reporter, "post", post, "spam");
  const note = `Dismissed for the log ${Date.now()}`;
  await apiCall(request, admin, "POST", `/admin/reports/${reportId}/dismiss`, { note });

  const adminPage = await pageFor(browser, admin);
  await adminPage.goto("/admin/decisions");
  await adminPage.getByRole("link", { name: "Reports → Show closed" }).click();
  await expect(adminPage).toHaveURL(/\/admin\/reports\?closed=1$/);
  await expect(adminPage.getByLabel("Show closed")).toBeChecked();
  await expect(adminPage.getByTestId("closed-report").filter({ hasText: note })).toBeVisible();
});
