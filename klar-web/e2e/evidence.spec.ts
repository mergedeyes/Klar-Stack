import { expect, test } from "@playwright/test";
import { adminSession, apiGet, pageFor, removedComment, signUp, uniqueName } from "./helpers";

// Removed likely-illegal content is kept as evidence and flagged for the
// authorities by the team's classification, not only by the report reason.

async function evidenceFor(request: import("@playwright/test").APIRequestContext, admin: Awaited<ReturnType<typeof adminSession>>, targetId: string) {
  const records = await apiGet<{ id: string; target_id: string }[]>(request, admin, "/admin/evidence");
  const record = records.find((r) => r.target_id === targetId);
  expect(record, "an evidence record for the removed comment").toBeTruthy();
  return record!.id;
}

/** Opens an evidence record; every opening needs a reason and is logged. */
async function openRecord(page: import("@playwright/test").Page, id: string) {
  await page.goto(`/admin/evidence/${id}`);
  await page.getByLabel("Why are you opening this record?").fill("Preparing a report to the authorities");
  await page.getByRole("button", { name: "Open", exact: true }).click();
}

test("a spam report found to be Holocaust denial is preserved, with a recommended report", async ({ browser, request }) => {
  const admin = await adminSession();
  const [author, reporter] = await Promise.all([signUp("denier"), signUp("reporter")]);
  const id = await removedComment(request, {
    author,
    reporter,
    admin,
    reason: "spam",
    violation: "extremism_promotion",
    text: `denial ${uniqueName("d")}`,
  });

  const adminPage = await pageFor(browser, admin);
  await openRecord(adminPage, await evidenceFor(request, admin, id));
  await expect(adminPage.getByText("Report to authorities: recommended")).toBeVisible();
});

test("an attack threat is flagged as a required report", async ({ browser, request }) => {
  const admin = await adminSession();
  const [author, reporter] = await Promise.all([signUp("threat"), signUp("reporter")]);
  const id = await removedComment(request, {
    author,
    reporter,
    admin,
    reason: "terrorism",
    violation: "terror_threat",
    text: `tomorrow at school ${uniqueName("t")}`,
  });

  const adminPage = await pageFor(browser, admin);
  await openRecord(adminPage, await evidenceFor(request, admin, id));
  await expect(adminPage.getByText("Report to authorities: required")).toBeVisible();
  await adminPage.goto("/admin/evidence");
  await expect(adminPage.getByText("Report to authorities: required").first()).toBeVisible();
});

test("a legal hold and a report to the authorities are recorded on the record, each in the audit trail", async ({ browser, request }) => {
  const admin = await adminSession();
  const [author, reporter] = await Promise.all([signUp("threat"), signUp("reporter")]);
  const id = await removedComment(request, {
    author,
    reporter,
    admin,
    reason: "terrorism",
    violation: "terror_threat",
    text: `at noon ${uniqueName("t")}`,
  });

  const adminPage = await pageFor(browser, admin);
  await openRecord(adminPage, await evidenceFor(request, admin, id));
  const trail = adminPage.locator("section", { hasText: "Audit trail" });
  await expect(trail.getByText("Opened", { exact: true })).toHaveCount(1);

  const holdReason = `Request from Staatsanwaltschaft ${uniqueName("az")}`;
  await adminPage.getByPlaceholder("Reason (e.g. request from Staatsanwaltschaft, Az. …)").fill(holdReason);
  await adminPage.getByRole("button", { name: "Set hold" }).click();
  await expect(adminPage.getByRole("button", { name: "Lift hold" })).toBeVisible();

  const today = new Date().toISOString().slice(0, 10);
  const record = adminPage.getByRole("button", { name: "Record report" });
  await adminPage.getByPlaceholder("Authority (e.g. BKA, jugendschutz.net)").fill("BKA");
  await adminPage.locator('input[type="date"]').fill(today);
  await adminPage.getByPlaceholder("Their reference / case number (optional)").fill("ST-4711");
  await record.click();

  await expect(trail.getByText("Legal hold set")).toHaveCount(1);
  await expect(trail.getByText(`“${holdReason}”`)).toBeVisible();
  await expect(trail.getByText("Reported to authority")).toHaveCount(1);
  await expect(trail.getByText(`BKA on ${today} · ref. ST-4711`)).toBeVisible();
  // Reopened later (logged again), the required report shows as done.
  await openRecord(adminPage, await evidenceFor(request, admin, id));
  await expect(adminPage.getByText("Reported to authorities", { exact: true })).toBeVisible();
  await expect(adminPage.getByText("Report to authorities: required")).toBeHidden();
  await expect(trail.getByText("Opened", { exact: true })).toHaveCount(2);
});
