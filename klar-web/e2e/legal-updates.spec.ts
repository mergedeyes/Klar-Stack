import { expect, test } from "@playwright/test";
import { adminSession, apiGet, apiJson, signIn, signUp, uniqueName, withDb } from "./helpers";

// Notices about changed Terms: shown once to accounts that existed before,
// accepted in the app, not shown to accounts created after. Runs in its own
// project after the others (playwright.config.ts), and removes its notices
// again, since they would cover the app for every older account.

// In order: the second test's account is created after the first notice,
// so it only sees its own.
test.describe.configure({ mode: "serial" });

const published: string[] = [];

test.afterAll(async () => {
  if (published.length === 0) return;
  await withDb((db) => db.query("DELETE FROM legal_updates WHERE id = ANY($1)", [published]));
});

test("existing accounts accept changed Terms once; new accounts never see the notice", async ({ page, request }) => {
  const admin = await adminSession();
  const existing = await signUp("existing");
  const summary = `Neue Punkte für Gewalt und sexuelle Inhalte ${uniqueName("s")}`;
  const { id } = await apiJson<{ id: string }>(request, admin, "POST", "/admin/legal-updates", {
    documents: ["terms"],
    summary,
  });
  published.push(id);

  await signIn(page, existing);
  await page.goto("/feed");
  const dialog = page.getByRole("dialog", { name: "Wir haben unsere Nutzungsbedingungen geändert" });
  await expect(dialog).toBeVisible();
  await expect(dialog.getByText(summary)).toBeVisible();
  // A Terms change can only be accepted, not dismissed.
  await expect(dialog.getByRole("button", { name: "Verstanden" })).toHaveCount(0);

  // The new version can be read without the notice in the way.
  await dialog.getByRole("link", { name: "Nutzungsbedingungen" }).click();
  await expect(page).toHaveURL(/\/nutzungsbedingungen/);
  await expect(page.getByRole("dialog")).toHaveCount(0);

  await page.goto("/feed");
  await dialog.getByRole("button", { name: "Zustimmen" }).click();
  await expect(dialog).toBeHidden();
  await page.reload();
  await expect(page.getByRole("heading", { name: /Wir haben unsere/ })).toHaveCount(0);

  const updates = await apiGet<{ id: string; acknowledged: number }[]>(request, admin, "/admin/legal-updates");
  expect(updates.find((u) => u.id === id)?.acknowledged).toBeGreaterThanOrEqual(1);

  // Created after it: accepted the new version at sign-up.
  const newcomer = await signUp("newcomer");
  const pending = await apiGet<unknown[]>(request, newcomer, "/legal-updates/pending");
  expect(pending).toHaveLength(0);
});

test("a privacy notice is only acknowledged", async ({ page, request }) => {
  const admin = await adminSession();
  const existing = await signUp("privacy");
  const { id } = await apiJson<{ id: string }>(request, admin, "POST", "/admin/legal-updates", {
    documents: ["privacy"],
    summary: `Wir beschreiben jetzt die Kontoprüfung ${uniqueName("p")}`,
  });
  published.push(id);

  await signIn(page, existing);
  await page.goto("/feed");
  const dialog = page.getByRole("dialog", { name: "Wir haben unsere Datenschutzerklärung geändert" });
  await expect(dialog).toBeVisible();
  await dialog.getByRole("button", { name: "Verstanden" }).click();
  await expect(page.getByRole("dialog")).toHaveCount(0);
});
