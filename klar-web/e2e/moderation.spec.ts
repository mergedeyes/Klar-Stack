import { expect, test } from "@playwright/test";
import {
  adminSession,
  apiGet,
  comment,
  pageFor,
  removedComment,
  report,
  signIn,
  signUp,
  uniqueName,
  upload,
} from "./helpers";

// Reporting, classifying a removal from the violation catalog, and what the
// author sees afterwards: points, the strike, and the statement of reasons.

test("a comment reported in the app is classified on removal and the author sees points and statement", async ({ page, browser, request }) => {
  const admin = await adminSession();
  const [author, reporter] = await Promise.all([signUp("author"), signUp("reporter")]);
  const post = await upload(request, reporter, "My holiday");
  const text = `you are an idiot ${uniqueName("c")}`;
  await comment(request, author, post, text);

  // The reporter reports it from the post page and sees the new reasons.
  await signIn(page, reporter);
  await page.goto(`/posts/${post}`);
  await page.getByRole("button", { name: "Report comment" }).click();
  for (const reason of [
    "Extremism or glorifying Nazism/fascism",
    "Intimate images shared without consent",
    "Terrorism or threats of serious violence",
    "Scam or fraud",
    "Selling drugs, weapons or other illegal goods",
  ]) {
    await expect(page.getByLabel(reason)).toBeVisible();
  }
  await page.getByLabel("Harassment or bullying").check();
  await page.getByRole("button", { name: "Submit report" }).click();
  await expect(page.getByText("Thanks — we'll review this.")).toBeVisible();

  // The admin finds it in the queue, classified by default as the
  // harassment type with the smallest points, criterion shown.
  const adminPage = await pageFor(browser, admin);
  await adminPage.goto("/admin/reports");
  const card = adminPage.locator("div.rounded-xl").filter({ hasText: text });
  await expect(card).toBeVisible();
  const picker = card.locator("select");
  await expect(picker).toHaveValue("insult");
  await expect(card.getByText("Eine einzelne herabsetzende Äußerung")).toBeVisible();

  // Another reason than the reporter's needs a justification first.
  const remove = card.getByRole("button", { name: "Remove content" });
  await picker.selectOption("terror_propaganda");
  await expect(card.getByRole("textbox", { name: "Justification" })).toBeVisible();
  await expect(remove).toBeDisabled();
  await card.getByRole("textbox", { name: "Justification" }).fill("Checking the rule");
  await expect(remove).toBeEnabled();
  // Back to the reporter's reason: no justification needed.
  await picker.selectOption("harassment_targeted");
  await expect(card.getByRole("textbox", { name: "Justification" })).toBeHidden();
  await remove.click();
  await expect(card).toBeHidden();

  // The author sees the points, the strike and the statement.
  await signIn(page, author);
  await page.goto("/moderation");
  await expect(page.getByText("Account status")).toBeVisible();
  await expect(page.getByText("/ 100 points")).toBeVisible();
  await expect(page.getByRole("meter", { name: "Account standing score" })).toHaveAttribute("aria-valuenow", "20");
  await page.getByRole("link", { name: /Targeted or repeated harassment/ }).click();
  await expect(page.getByText(/Eingestuft als „Gezielte oder wiederholte Belästigung“/)).toBeVisible();
  await expect(page.getByText(/20 Punkte angerechnet/)).toBeVisible();
  await expect(page.getByText("Checking the rule")).toBeHidden();
});

test("the third strike for the same reason within 30 days is marked as a repeat", async ({ page, request }) => {
  const admin = await adminSession();
  const [author, reporter] = await Promise.all([signUp("repeat"), signUp("reporter")]);
  for (let i = 0; i < 3; i++) {
    await removedComment(request, { author, reporter, admin });
  }
  await signIn(page, author);
  await page.goto("/moderation");
  await expect(page.getByRole("meter", { name: "Account standing score" })).toHaveAttribute("aria-valuenow", "18");
  await expect(page.getByText("(repeat, ×1.5)")).toHaveCount(1);
  await expect(page.getByText("+8", { exact: true })).toBeVisible();
});

test("dismissing a report takes it out of the queue and lifts the automatic warning", async ({ page, browser, request }) => {
  const admin = await adminSession();
  const [author, reporter] = await Promise.all([signUp("warned"), signUp("reporter")]);
  const caption = `violent ${uniqueName("p")}`;
  const post = await upload(request, author, caption);
  await report(request, reporter, "post", post, "violence");

  // Shown behind a warning until it is reviewed.
  await signIn(page, reporter);
  await page.goto(`/posts/${post}`);
  await expect(page.getByText(/may violate our guidelines|content warning/i).first()).toBeVisible();

  const adminPage = await pageFor(browser, admin);
  await adminPage.goto("/admin/reports");
  const card = adminPage.locator("div.rounded-xl").filter({ hasText: caption });
  await card.getByRole("button", { name: "Dismiss" }).click();
  await expect(card).toBeHidden();

  const decisions = await apiGet<{ restriction: string; lifted_at: string | null }[]>(request, author, "/moderation/decisions");
  expect(decisions[0].lifted_at).not.toBeNull();
});

test("the admin sees the removed content of a strike with its context", async ({ browser, request }) => {
  const admin = await adminSession();
  const [author, reporter] = await Promise.all([signUp("context"), signUp("reporter")]);
  const text = `nobody asked ${uniqueName("c")}`;
  await removedComment(request, { author, reporter, admin, violation: "threat_stalking", text });

  const adminPage = await pageFor(browser, admin);
  await adminPage.goto(`/admin/standing`);
  await adminPage.getByPlaceholder("Look up an account by username").fill(author.username);
  await adminPage.getByRole("button", { name: "Look up" }).click();
  const card = adminPage.getByRole("region", { name: "Looked-up account" });
  await expect(card).toContainText(`@${author.username}`);
  await expect(card.getByText(text)).toBeHidden();
  // Expanding right after the lookup can race hydration; retry the click.
  const show = card.getByRole("button", { name: "Show content" });
  await expect(async () => {
    if (!(await show.isVisible())) await card.locator("summary", { hasText: "active strike" }).click();
    await expect(show).toBeVisible({ timeout: 1000 });
  }).toPass();
  await show.click();
  await expect(card.getByText(text)).toBeVisible();
  await expect(card.getByText(`On a post by @${reporter.username}`)).toBeVisible();
  await expect(card.getByText(/Criterion: Androhung von Gewalt/)).toBeVisible();
});
