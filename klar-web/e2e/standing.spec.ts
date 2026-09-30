import { expect, test } from "@playwright/test";
import {
  adminSession,
  apiCall,
  apiGet,
  pageFor,
  removedComment,
  signIn,
  signUp,
  uniqueName,
  upload,
} from "./helpers";

// Account standing: suggested measures, warnings, suspensions and what a
// suspended account can and can't do, objections against measures.

async function lookUp(page: import("@playwright/test").Page, username: string) {
  await page.goto("/admin/standing");
  await page.getByPlaceholder("Look up an account by username").fill(username);
  await page.getByRole("button", { name: "Look up" }).click();
  // The lookup result, not the same account in the "Needs attention" list.
  const result = page.getByRole("region", { name: "Looked-up account" });
  await expect(result).toContainText(`@${username}`);
  return result;
}

test("an account over the threshold gets a suggested warning, and applying it clears the suggestion", async ({ browser, request }) => {
  const admin = await adminSession();
  const [author, reporter] = await Promise.all([signUp("warn"), signUp("reporter")]);
  await removedComment(request, { author, reporter, admin, violation: "threat_stalking" });

  const adminPage = await pageFor(browser, admin);
  const card = await lookUp(adminPage, author.username);
  await expect(card.getByText("Suggested: Warning")).toBeVisible();
  await expect(card.getByLabel("Measure")).toHaveValue("warning");
  await expect(card.getByLabel("Main reason")).toHaveValue("harassment");
  await card.getByRole("button", { name: "Apply" }).click();
  await expect(card.getByText("Suggested: Warning")).toBeHidden();
  await expect(card.getByText("1 earlier measure")).toBeVisible();

  // The overview lists it too (40 points is over the warning threshold).
  await adminPage.goto("/admin/standing");
  await expect(adminPage.getByRole("link", { name: `@${author.username}` })).toBeVisible();
});

test("glorifying extremism is grave: a warning first, a permanent suspension on the second case", async ({ browser, request }) => {
  const admin = await adminSession();
  const [author, reporter] = await Promise.all([signUp("grave"), signUp("reporter")]);
  await removedComment(request, { author, reporter, admin, reason: "extremism", violation: "extremism_glorifying" });

  const adminPage = await pageFor(browser, admin);
  let card = await lookUp(adminPage, author.username);
  await expect(card.getByRole("meter", { name: "Account standing score" })).toHaveAttribute("aria-valuenow", "60");
  await expect(card.getByText("Suggested: Warning")).toBeVisible();
  await card.getByRole("button", { name: "Apply" }).click();
  await expect(card.getByText("1 earlier measure")).toBeVisible();

  await removedComment(request, { author, reporter, admin, reason: "extremism", violation: "extremism_glorifying" });
  card = await lookUp(adminPage, author.username);
  await expect(card.getByText("Suggested: Suspend permanently")).toBeVisible();
});

test("a suspended account is read-only and hidden, until the suspension is lifted", async ({ page, browser, request }) => {
  const admin = await adminSession();
  const [author, other] = await Promise.all([signUp("suspended"), signUp("other")]);
  const otherPost = await upload(request, other, "Other's post");
  await apiCall(request, admin, "POST", `/admin/users/${author.username}/measures`, { measure: "suspend_7d", reason: "harassment" });

  // The suspended user sees why and what they can still do...
  await signIn(page, author);
  await page.goto("/moderation");
  await expect(page.getByText(/Your account is suspended until/)).toBeVisible();
  await expect(page.getByText(/export your data and delete your account/)).toBeVisible();
  await expect(page.getByRole("link", { name: /Suspended · 7 days · account/ })).toBeVisible();

  // ...and can't comment: the reason shows under the field, the text stays.
  await page.goto(`/posts/${otherPost}`);
  const field = page.getByLabel("Add a comment");
  await field.fill("Can I still post?");
  await page.getByRole("button", { name: "Post comment" }).click();
  await expect(page.getByRole("alert").filter({ hasText: "suspended until" })).toBeVisible();
  await expect(field).toHaveValue("Can I still post?");

  // Hidden from others: the profile doesn't exist for them.
  const otherPage = await pageFor(browser, other);
  await otherPage.goto(`/users/${author.username}`);
  await expect(otherPage).toHaveURL(/\/feed$/);

  // Lifted early: visible again, and commenting works.
  const adminPage = await pageFor(browser, admin);
  const card = await lookUp(adminPage, author.username);
  await expect(card.getByText(/Suspended until/)).toBeVisible();
  await card.getByRole("button", { name: "Lift suspension" }).click();
  await expect(card.getByText(/Suspended until/)).toBeHidden();

  await otherPage.goto(`/users/${author.username}`);
  await expect(otherPage.getByText(`@${author.username}`).first()).toBeVisible();
  await page.reload();
  await field.fill("Back again");
  await page.getByRole("button", { name: "Post comment" }).click();
  await expect(page.getByText("Back again")).toBeVisible();
});

test("a permanent suspension shows when the account will be deleted", async ({ page, request }) => {
  const admin = await adminSession();
  const author = await signUp("banned");
  await apiCall(request, admin, "POST", `/admin/users/${author.username}/measures`, { measure: "ban", reason: "spam" });
  await signIn(page, author);
  await page.goto("/moderation");
  await expect(page.getByText("Your account is permanently suspended.")).toBeVisible();
  await expect(page.getByText(/will be deleted on/)).toBeVisible();
});

test("an accepted objection ends a suspension", async ({ page, browser, request }) => {
  const admin = await adminSession();
  const author = await signUp("objects");
  await apiCall(request, admin, "POST", `/admin/users/${author.username}/measures`, { measure: "suspend_30d", reason: "harassment" });
  const [decision] = await apiGet<{ id: string }[]>(request, author, "/moderation/decisions");

  await signIn(page, author);
  await page.goto(`/moderation/decisions/${decision.id}`);
  await expect(page.getByText("Vorübergehend gesperrt", { exact: false })).toBeVisible();
  const objection = `This was my brother ${uniqueName("o")}`;
  await page.getByPlaceholder("Warum ist die Entscheidung aus deiner Sicht falsch?").fill(objection);
  await page.getByRole("button", { name: "Widerspruch senden" }).click();
  await expect(page.getByText("Unser Team prüft deinen Widerspruch")).toBeVisible();

  const adminPage = await pageFor(browser, admin);
  await adminPage.goto("/admin/moderation");
  const card = adminPage.locator("div.rounded-xl").filter({ hasText: objection });
  await card.getByPlaceholder("Response to the user (required)").fill("Understood, lifted.");
  await card.getByRole("button", { name: "Accept" }).click();
  await expect(card).toBeHidden();

  await page.reload();
  await expect(page.getByText("Wir haben deinem Widerspruch stattgegeben.")).toBeVisible();
  const standing = await apiGet<{ suspension: unknown }>(request, author, "/users/me/standing");
  expect(standing.suspension).toBeNull();
});
