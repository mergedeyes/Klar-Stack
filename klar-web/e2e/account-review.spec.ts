import { expect, test } from "@playwright/test";
import {
  adminSession,
  API,
  apiCall,
  apiGet,
  comment,
  mutualFollow,
  pageFor,
  report,
  signUp,
  uniqueName,
  upload,
} from "./helpers";

// Reviewing an account's recent activity: the "Needs review" list, the
// logged review page, and its three decisions.

test("a burst of comments puts the account under Needs review, and the review shows its activity", async ({ browser, request }) => {
  const admin = await adminSession();
  const [spammer, victim] = await Promise.all([signUp("burst"), signUp("victim")]);
  const post = await upload(request, victim, "Holiday");
  for (let i = 0; i < 12; i++) {
    await comment(request, spammer, post, "Cheap coins at https://scam.example");
  }
  await mutualFollow(request, spammer, victim);
  const secret = `secret private words ${uniqueName("m")}`;
  await apiCall(request, spammer, "POST", "/chats/send", { receiver_id: victim.id, body: secret });

  const adminPage = await pageFor(browser, admin);
  await adminPage.goto("/admin/security");
  const candidate = adminPage.getByRole("link").filter({ hasText: `@${spammer.username}` });
  await expect(candidate).toContainText("Burst of activity");
  await expect(candidate).toContainText("Same text repeated");
  await candidate.click();

  // The reason comes from the signals; opening needs one.
  await expect(adminPage).toHaveURL(new RegExp(`/admin/review/${spammer.username}`));
  const reason = adminPage.getByLabel("Reason");
  await expect(reason).toHaveValue(/Signals: Burst of activity/);
  await reason.fill("");
  await expect(adminPage.getByRole("button", { name: "Open review" })).toBeDisabled();
  await reason.fill("Signals in the security tab");
  await adminPage.getByRole("button", { name: "Open review" }).click();

  await expect(adminPage.getByText("Most posts, comments and messages in 10 minutes:")).toBeVisible();
  await expect(adminPage.getByText("Comments (12)")).toBeVisible();
  await adminPage.getByText("Comments (12)").click();
  await expect(adminPage.getByText("Cheap coins at https://scam.example").first()).toBeVisible();
  await expect(adminPage.getByText(/sent to/).first()).toBeVisible();
  // Direct messages as numbers only.
  await expect(adminPage.getByText(secret)).toHaveCount(0);

  // "No action" needs a note.
  const noAction = adminPage.getByRole("button", { name: "No action" });
  await expect(noAction).toBeDisabled();
  await adminPage.getByLabel("Note").fill("A friend's account, they confirmed it's them");
  await noAction.click();
  await expect(adminPage.getByText("Decided: No action.")).toBeVisible();
});

test("a review started from a spam report can lock the account", async ({ browser, request }) => {
  const admin = await adminSession();
  const [hijacked, reporter] = await Promise.all([signUp("hijacked"), signUp("reporter")]);
  const post = await upload(request, reporter, "a post");
  const text = `buy followers ${uniqueName("s")}`;
  const commentId = await comment(request, hijacked, post, text);
  await report(request, reporter, "comment", commentId, "spam");

  const adminPage = await pageFor(browser, admin);
  await adminPage.goto("/admin/reports");
  const card = adminPage.locator("div.rounded-xl").filter({ hasText: text });
  await expect(card.getByText("check whether it was taken over or is a bot")).toBeVisible();
  await card.getByRole("link", { name: "Review account" }).click();

  await expect(adminPage.getByLabel("Reason")).toHaveValue("Report: Spam");
  await adminPage.getByRole("button", { name: "Open review" }).click();
  await expect(adminPage.getByText("Reports about this account (1)")).toBeVisible();
  const note = `Never posted ads before; looks hijacked ${uniqueName("n")}`;
  await adminPage.getByLabel("Note").fill(note);
  await adminPage.getByRole("button", { name: "Lock as possibly hacked" }).click();
  await expect(adminPage.getByText("Decided: Lock as possibly hacked.")).toBeVisible();

  const res = await request.get(`${API}/users/me`, { headers: { Authorization: `Bearer ${hijacked.access_token}` } });
  expect(res.status()).toBe(401);
  await adminPage.goto("/admin/security");
  await expect(adminPage.getByText(note)).toBeVisible();
});

test("a review can suspend a bot permanently, with a statement that says why", async ({ browser, request }) => {
  const admin = await adminSession();
  const bot = await signUp("bot");
  const adminPage = await pageFor(browser, admin);
  await adminPage.goto(`/admin/review/${bot.username}`);
  await adminPage.getByLabel("Reason").fill("Found while browsing");
  await adminPage.getByRole("button", { name: "Open review" }).click();
  await adminPage.getByLabel("Note").fill("Posts the same link every 30 seconds");
  await adminPage.getByRole("button", { name: "Bot or spam-only account" }).click();
  await expect(adminPage.getByText("Decided: Bot or spam-only account.")).toBeVisible();

  const standing = await apiGet<{ suspension: { permanent: boolean } }>(request, bot, "/users/me/standing");
  expect(standing.suspension.permanent).toBe(true);
  const decisions = await apiGet<{ explanation: string }[]>(request, bot, "/moderation/decisions");
  expect(decisions[0].explanation).toContain("automatisiert betrieben");
  expect(JSON.stringify(decisions)).not.toContain("every 30 seconds");
});
