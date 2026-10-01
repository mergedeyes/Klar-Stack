import { expect, test, type Page } from "@playwright/test";
import { adminSession, apiCall, apiGet, pageFor, report, signIn, signUp, trustedSignUp, uniqueName, upload } from "./helpers";

// Reporting posts and profiles from the app, the two reasons that hide a
// post at once (CSAM, intimate images), the statement held back for CSAM
// until an admin sends it, and objecting to a removal.

async function reportFromPage(page: Page, button: "Report post" | "Report user", reason: string) {
  await page.getByRole("button", { name: button }).click();
  await page.getByLabel(reason).check();
  await page.getByRole("button", { name: "Submit report" }).click();
  await expect(page.getByText("Thanks — we'll review this.")).toBeVisible();
}

test("posts and profiles reported in the app reach the queue, and the reporter sees the outcome", async ({ page, browser, request }) => {
  const admin = await adminSession();
  const [author, reporter] = await Promise.all([signUp("spammer"), signUp("reporter")]);
  const caption = `buy followers ${uniqueName("p")}`;
  const post = await upload(request, author, caption);

  await signIn(page, reporter);
  await page.goto(`/posts/${post}`);
  await reportFromPage(page, "Report post", "Spam");
  await page.goto(`/users/${author.username}`);
  await reportFromPage(page, "Report user", "Impersonation");

  const adminPage = await pageFor(browser, admin);
  await adminPage.goto("/admin/reports");
  const postCard = adminPage.locator("div.rounded-xl").filter({ hasText: caption });
  await expect(postCard).toContainText("Spam");
  const userCard = adminPage
    .locator("div.rounded-xl")
    .filter({ hasText: "Impersonation" })
    .filter({ has: adminPage.getByRole("link", { name: author.username, exact: true }) });
  await expect(userCard).toContainText(`Reported by ${reporter.username}`);
  // A live account can't be removed from the queue, only reviewed.
  await expect(userCard.getByRole("button", { name: /Remove content|Confirm violation/ })).toHaveCount(0);
  await expect(userCard.getByRole("link", { name: "Review account" })).toBeVisible();

  await postCard.getByRole("button", { name: "Remove content" }).click();
  await expect(postCard).toBeHidden();
  await userCard.getByRole("button", { name: "Dismiss" }).click();
  await expect(userCard).toBeHidden();

  // The reporter learns the outcome of each, nothing about the person.
  await page.goto("/moderation#reports");
  const reports = page.locator("#reports ~ div");
  await expect(reports.getByText("Reviewed — the content was removed")).toHaveCount(1);
  await expect(reports.getByText("Reviewed — no violation found")).toHaveCount(1);
});

test("a CSAM report hides the post at once, and its statement waits until an admin sends it", async ({ page, browser, request }) => {
  const admin = await adminSession();
  // A reporter whose account is verified and older than a day: a brand-new
  // account's report only queues.
  const [author, reporter, bystander] = await Promise.all([signUp("uploader"), trustedSignUp("reporter"), signUp("bystander")]);
  const caption = `held back ${uniqueName("p")}`;
  const post = await upload(request, author, caption);

  await signIn(page, reporter);
  await page.goto(`/posts/${post}`);
  await reportFromPage(page, "Report post", "Child sexual abuse material");

  // Gone for everyone else right away, still there for its author.
  const other = await pageFor(browser, bystander);
  await other.goto(`/posts/${post}`);
  await expect(other.getByText("This post isn't available")).toBeVisible();
  const own = await pageFor(browser, author);
  await own.goto(`/posts/${post}`);
  await expect(own.getByText(caption)).toBeVisible();

  // Removed by the team: the author isn't told yet.
  const queue = await apiGet<{ target_id: string; reports: { id: string }[] }[]>(request, admin, "/admin/reports");
  const reportId = queue.find((g) => g.target_id === post)!.reports[0].id;
  await apiCall(request, admin, "POST", `/admin/reports/${reportId}/remove`, {});
  expect(await apiGet<unknown[]>(request, author, "/moderation/decisions")).toEqual([]);

  const adminPage = await pageFor(browser, admin);
  await adminPage.goto("/admin/moderation");
  const held = adminPage.locator("div.rounded-xl").filter({ hasText: caption });
  // The automatic hide is replaced by the removal, so only one card.
  await expect(held).toHaveCount(1);
  await held.getByRole("button", { name: "Send statement" }).click();
  await expect(held).toBeHidden();

  await own.goto("/moderation");
  await own.getByRole("link", { name: new RegExp(`Removed · post ${caption}`) }).click();
  await expect(own.getByText(/§ 184b StGB/)).toBeVisible();
});

test("an intimate-images report hides the post at once and tells its author right away", async ({ browser, request }) => {
  const [author, reporter, bystander] = await Promise.all([signUp("poster"), trustedSignUp("reporter"), signUp("bystander")]);
  const caption = `ncii ${uniqueName("p")}`;
  const post = await upload(request, author, caption);
  await report(request, reporter, "post", post, "ncii");

  const other = await pageFor(browser, bystander);
  await other.goto(`/posts/${post}`);
  await expect(other.getByText("This post isn't available")).toBeVisible();

  // Not held back: the automatic hide has its statement at once.
  const own = await pageFor(browser, author);
  await own.goto("/moderation");
  await own.getByRole("link", { name: /Hidden · post/ }).click();
  await expect(own.getByText(/automatisch ausgeblendet, bis unser Team ihn geprüft hat/)).toBeVisible();
});

test("the author objects to a removal and reads the team's answer", async ({ page, browser, request }) => {
  const admin = await adminSession();
  const [author, reporter] = await Promise.all([signUp("objector"), signUp("reporter")]);
  const caption = `joke ${uniqueName("p")}`;
  const post = await upload(request, author, caption);
  const reportId = await report(request, reporter, "post", post, "harassment");
  await apiCall(request, admin, "POST", `/admin/reports/${reportId}/remove`, {});

  await signIn(page, author);
  await page.goto("/moderation");
  await page.getByRole("link", { name: /Removed · post/ }).click();
  const objection = `It was a joke between friends ${uniqueName("o")}`;
  await page.getByPlaceholder("Warum ist die Entscheidung aus deiner Sicht falsch?").fill(objection);
  await page.getByRole("button", { name: "Widerspruch senden" }).click();
  await expect(page.getByText("Unser Team prüft deinen Widerspruch")).toBeVisible();
  // Once per decision.
  await expect(page.getByRole("button", { name: "Widerspruch senden" })).toHaveCount(0);

  const adminPage = await pageFor(browser, admin);
  await adminPage.goto("/admin/moderation");
  const card = adminPage.locator("div.rounded-xl").filter({ hasText: objection });
  await expect(card).toContainText(caption);
  await card.getByPlaceholder("Response to the user (required)").fill("It targets a person; the removal stands.");
  await card.getByRole("button", { name: "Reject" }).click();
  await expect(card).toBeHidden();

  await page.reload();
  await expect(page.getByText("Wir haben deinen Widerspruch geprüft und halten an der Entscheidung fest.")).toBeVisible();
  await expect(page.getByText("It targets a person; the removal stands.")).toBeVisible();
  // The strike stays.
  const standing = await apiGet<{ score: number }>(request, author, "/users/me/standing");
  expect(standing.score).toBe(5);
});

test("the report dialog helps in a crisis, offers to block, and a dismissed report can be sent back once", async ({ page, browser, request }) => {
  const admin = await adminSession();
  const [author, reporter] = await Promise.all([signUp("sad"), signUp("worried")]);
  const caption = `nothing matters ${uniqueName("p")}`;
  const post = await upload(request, author, caption);

  await signIn(page, reporter);
  await page.goto(`/posts/${post}`);
  await page.getByRole("button", { name: "Report post" }).click();
  await page.getByLabel("Self-harm or suicide").check();
  await expect(page.getByText(/0800 111 0 111/).first()).toBeVisible();
  await page.getByRole("button", { name: "Submit report" }).click();
  await expect(page.getByText("Thanks — we'll review this.")).toBeVisible();
  await page.getByRole("button", { name: `Block @${author.username}` }).click();
  await expect(page.getByText(new RegExp(`blocked @${author.username}`))).toBeVisible();
  await page.getByRole("button", { name: "Done" }).click();

  // Dismissed: the reporter asks once to look again, with a note.
  const queue = await apiGet<{ target_id: string; reports: { id: string }[] }[]>(request, admin, "/admin/reports");
  const reportId = queue.find((g) => g.target_id === post)!.reports[0].id;
  await apiCall(request, admin, "POST", `/admin/reports/${reportId}/dismiss`, {});
  await page.goto("/moderation#reports");
  await page.getByRole("button", { name: "Ask us to check again" }).click();
  await page.getByPlaceholder("What did we miss? (optional)").fill("Please look at the caption again.");
  await page.getByRole("button", { name: /Send — you can ask once/ }).click();
  await expect(page.getByText("Being checked again")).toBeVisible();
  await expect(page.getByRole("button", { name: "Ask us to check again" })).toHaveCount(0);

  const adminPage = await pageFor(browser, admin);
  await adminPage.goto("/admin/reports");
  const card = adminPage.getByTestId("report-group").filter({ hasText: caption });
  await expect(card.getByText("Re-check requested")).toBeVisible();
  await expect(card).toContainText("Please look at the caption again.");
});
