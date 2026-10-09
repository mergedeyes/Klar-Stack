import { expect, test } from "@playwright/test";
import { apiCall, pageFor, signIn, signUp } from "./helpers";

// The search result cards have a follow button, which has to start from the
// viewer's actual relationship to each account.

test("search shows accounts already followed or requested as such", async ({ page, request }) => {
  const [alice, bob, carol] = await Promise.all([signUp("searcher"), signUp("followed"), signUp("private")]);
  await apiCall(request, alice, "POST", `/users/${bob.username}/follow`);
  await apiCall(request, carol, "PATCH", "/users/me", { is_private: true });

  await signIn(page, alice);
  await page.goto("/search");
  const search = page.getByPlaceholder("Search people…");

  await search.fill(bob.username);
  await expect(page.getByRole("button", { name: "Following", exact: true })).toBeVisible();

  // A private account only gets a request, and the card says so.
  await search.fill(carol.username);
  await page.getByRole("button", { name: "Follow", exact: true }).click();
  await expect(page.getByRole("button", { name: "Requested", exact: true })).toBeVisible();
  await page.reload();
  await page.getByPlaceholder("Search people…").fill(carol.username);
  await expect(page.getByRole("button", { name: "Requested", exact: true })).toBeVisible();
});

test("a block hides both accounts from each other's search", async ({ page, browser, request }) => {
  const [alice, bob] = await Promise.all([signUp("blocker"), signUp("blocked")]);
  await apiCall(request, alice, "POST", `/users/${bob.username}/block`);

  await signIn(page, alice);
  await page.goto("/search");
  await page.getByPlaceholder("Search people…").fill(bob.username);
  await expect(page.getByText(`No users found for “${bob.username}”`)).toBeVisible();

  const other = await pageFor(browser, bob);
  await other.goto("/search");
  await other.getByPlaceholder("Search people…").fill(alice.username);
  await expect(other.getByText(`No users found for “${alice.username}”`)).toBeVisible();
});
