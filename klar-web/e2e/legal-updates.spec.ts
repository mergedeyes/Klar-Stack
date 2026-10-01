import { expect, test } from "@playwright/test";
import { adminSession, apiCall, apiGet, apiJson, pageFor, signIn, signUp, uniqueName, withDb } from "./helpers";

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

test("the admin list filters by date and sorts by acceptance and emails", async ({ browser, request }) => {
  const admin = await adminSession();
  const [reader, other] = await Promise.all([signUp("reader"), signUp("other")]);
  const accepted = `Viel akzeptiert ${uniqueName("a")}`;
  const ignored = `Kaum beachtet ${uniqueName("i")}`;
  const a = await apiJson<{ id: string }>(request, admin, "POST", "/admin/legal-updates", { documents: ["terms"], summary: accepted });
  const b = await apiJson<{ id: string }>(request, admin, "POST", "/admin/legal-updates", { documents: ["privacy"], summary: ignored });
  published.push(a.id, b.id);

  // "accepted" gets two acceptances, "ignored" none but more emails.
  await apiCall(request, reader, "POST", `/legal-updates/${a.id}/acknowledge`);
  await apiCall(request, other, "POST", `/legal-updates/${a.id}/acknowledge`);
  // Publishing starts a background run that emails every verified account
  // about each notice, one every 200 ms, so the real counts depend on how
  // far it got and on how many accounts earlier tests verified. Marking
  // every account as emailed about "ignored" puts it ahead for sure:
  // "accepted" can only ever reach the verified ones, and reader and other
  // aren't verified.
  await withDb((db) =>
    db.query("INSERT INTO legal_update_emails (update_id, user_id) SELECT $1, id FROM users ON CONFLICT DO NOTHING", [b.id]),
  );
  // The admin's own account existed before too: accept, so the notices
  // don't cover the page.
  const pending = await apiGet<{ id: string }[]>(request, admin, "/legal-updates/pending");
  for (const p of pending) await apiCall(request, admin, "POST", `/legal-updates/${p.id}/acknowledge`);

  const adminPage = await pageFor(browser, admin);
  await adminPage.goto("/admin/legal-updates");
  const order = async () => {
    const texts = await adminPage.locator("main div.rounded-xl").allInnerTexts();
    return [texts.findIndex((t) => t.includes(accepted)), texts.findIndex((t) => t.includes(ignored))];
  };
  const sortBy = adminPage.getByLabel("Sort by");

  // Newest first by default: "ignored" was published last.
  await expect(adminPage.getByText(ignored)).toBeVisible();
  let [ia, ii] = await order();
  expect(ii).toBeLessThan(ia);

  await sortBy.selectOption("most_accepted");
  [ia, ii] = await order();
  expect(ia).toBeLessThan(ii);
  await sortBy.selectOption("least_accepted");
  [ia, ii] = await order();
  expect(ii).toBeLessThan(ia);
  await sortBy.selectOption("most_emails");
  [ia, ii] = await order();
  expect(ii).toBeLessThan(ia);
  await sortBy.selectOption("least_emails");
  [ia, ii] = await order();
  expect(ia).toBeLessThan(ii);

  // A range in the future shows nothing; Reset brings everything back.
  // A fixed far-future day: "tomorrow" computed in UTC can still be today
  // in the admin's time zone.
  await adminPage.getByLabel("From").fill("2099-01-01");
  await expect(adminPage.getByText("No notice published in this date range.")).toBeVisible();
  await expect(adminPage.getByText(accepted)).toHaveCount(0);
  await adminPage.getByRole("button", { name: "Reset" }).click();
  await expect(adminPage.getByText(accepted)).toBeVisible();
  await expect(sortBy).toHaveValue("latest");
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
