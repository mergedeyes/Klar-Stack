import { expect, test } from "@playwright/test";
import { adminSession, apiCall, pageFor, signIn, signUp, uniqueName, type Session } from "./helpers";
import type { APIRequestContext } from "@playwright/test";

// Chats show the list and the open conversation side by side on a wide
// screen; a phone has room for one of them at a time, with a way back.

async function conversationBetween(request: APIRequestContext): Promise<[Session, Session]> {
  const alice = await signUp("alice");
  const bob = await signUp("bob");
  // Messaging needs a mutual follow.
  await apiCall(request, alice, "POST", `/users/${bob.username}/follow`);
  await apiCall(request, bob, "POST", `/users/${alice.username}/follow`);
  await apiCall(request, bob, "POST", "/chats/send", { receiver_id: alice.id, body: "Hallo Alice" });
  return [alice, bob];
}

test("opening a chat shows the conversation", async ({ page, request }, testInfo) => {
  const [alice, bob] = await conversationBetween(request);
  await signIn(page, alice);
  await page.goto("/chats");

  const listHeading = page.getByRole("heading", { name: "Chats" });
  await page.getByRole("button", { name: new RegExp(bob.username) }).click();
  // The last match is the message itself; on a wide screen the list's
  // preview of it comes first.
  await expect(page.getByText("Hallo Alice").last()).toBeVisible();
  await expect(page.getByPlaceholder(`Message @${bob.username}...`)).toBeVisible();

  // Message actions fade in on hover; a touch screen can't hover, so there
  // they are always shown. (Playwright counts opacity 0 as visible, hence
  // the explicit check.)
  const actions = page.getByRole("button", { name: "Reply" }).locator("..");
  const back = page.getByRole("button", { name: "Back to chats" });
  if (testInfo.project.name === "phone") {
    await expect(actions).toHaveCSS("opacity", "1");
    await expect(listHeading).toBeHidden();
    await back.click();
    await expect(listHeading).toBeVisible();
    await expect(page.getByPlaceholder(`Message @${bob.username}...`)).toBeHidden();
  } else {
    await expect(actions).toHaveCSS("opacity", "0");
    await expect(listHeading).toBeVisible();
    await expect(back).toBeHidden();
  }
});

test("the chat list uses the whole width on a phone", async ({ page, request }, testInfo) => {
  test.skip(testInfo.project.name !== "phone", "phone layout only");
  const [alice, bob] = await conversationBetween(request);
  await signIn(page, alice);
  await page.goto("/chats");

  const row = page.getByRole("button", { name: new RegExp(bob.username) });
  await expect(row).toBeVisible();
  const box = await row.boundingBox();
  const width = page.viewportSize()!.width;
  // The row spans the screen apart from the list's own small padding.
  expect(box!.width).toBeGreaterThan(width - 32);
});

test("a reported message reaches the team only through its evidence record, with the messages before it", async ({ page, browser, request }) => {
  const admin = await adminSession();
  const [alice, bob] = await conversationBetween(request);
  const threat = `I know where you live ${uniqueName("m")}`;
  await apiCall(request, bob, "POST", "/chats/send", { receiver_id: alice.id, body: threat });

  await signIn(page, alice);
  await page.goto("/chats");
  await page.getByRole("button", { name: new RegExp(bob.username) }).click();
  await page.getByRole("button", { name: "Report message" }).last().click();
  await expect(page.getByText(/read it and the ten messages before it/)).toBeVisible();
  await page.getByLabel("Harassment or bullying").check();
  await page.getByRole("button", { name: "Submit report" }).click();
  await expect(page.getByText("Thanks — we'll review this.")).toBeVisible();
  await expect(page.getByRole("button", { name: `Block @${bob.username}` })).toBeVisible();
  await page.getByRole("button", { name: "Done" }).click();

  // The queue names the sender, never the text.
  const adminPage = await pageFor(browser, admin);
  await adminPage.goto("/admin/reports");
  const card = adminPage
    .getByTestId("report-group")
    .filter({ hasText: "Direct message" })
    .filter({ has: adminPage.getByRole("link", { name: bob.username, exact: true }) });
  await expect(card).toBeVisible();
  await expect(card).not.toContainText(threat);

  // Opening it takes a reason and shows the message with what came before.
  await card.getByRole("link", { name: /Open the message and the ten before it/ }).click();
  await adminPage.getByLabel("Why are you opening this record?").fill("Reviewing a report");
  await adminPage.getByRole("button", { name: "Open", exact: true }).click();
  await expect(adminPage.getByText(threat)).toBeVisible();
  await adminPage.getByText("The 1 message before it").click();
  await expect(adminPage.getByText("Hallo Alice")).toBeVisible();
});
