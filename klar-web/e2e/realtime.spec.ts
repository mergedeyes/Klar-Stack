import { expect, test, type Page } from "@playwright/test";
import { apiCall, signIn, signUp, upload } from "./helpers";

// Live updates over the notification stream: a like reaches the bell and a
// message reaches the open chat without reloading.

/** Opens `path` and waits until the notification stream is connected (the
 * server subscribes before it answers), so no event is sent too early. */
async function openWithStream(page: Page, path: string) {
  const stream = page.waitForResponse((res) => res.url().includes("/notifications/stream?"));
  await page.goto(path);
  await stream;
}

test("a like shows up in the bell without reloading", async ({ page, request }) => {
  const [alice, bob] = await Promise.all([signUp("alice"), signUp("bob")]);
  const post = await upload(request, alice, "live post");
  await signIn(page, alice);
  await openWithStream(page, "/feed");

  await apiCall(request, bob, "POST", `/posts/${post}/like`);
  const bell = page.getByRole("button", { name: "Notifications (1 unread)" });
  await expect(bell).toBeVisible();
  await bell.click();
  await expect(page.getByText(`${bob.username} liked your post`)).toBeVisible();
});

test("a new message appears in the open chat without reloading", async ({ page, request }) => {
  const [alice, bob] = await Promise.all([signUp("alice"), signUp("bob")]);
  await apiCall(request, alice, "POST", `/users/${bob.username}/follow`);
  await apiCall(request, bob, "POST", `/users/${alice.username}/follow`);
  await apiCall(request, bob, "POST", "/chats/send", { receiver_id: alice.id, body: "first message" });

  await signIn(page, alice);
  await openWithStream(page, "/chats");
  await page.getByRole("button", { name: new RegExp(bob.username) }).click();
  await expect(page.getByText("first message").last()).toBeVisible();

  await apiCall(request, bob, "POST", "/chats/send", { receiver_id: alice.id, body: "second, live" });
  await expect(page.getByText("second, live").last()).toBeVisible();
});
