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
