import { expect, test } from "@playwright/test";
import { adminSession, pageFor, signUp, uniqueName, upload } from "./helpers";

// A rights claim from someone without an account, through the public form:
// the status link, a request for evidence, acceptance (the post is hidden,
// not deleted), and the uploader's successful objection restoring it.

test("a claim from the public form is answered, accepted, objected to and restored", async ({ browser, request, baseURL }) => {
  const admin = await adminSession();
  const [uploader, bystander] = await Promise.all([signUp("uploader"), signUp("bystander")]);
  const caption = `sunset ${uniqueName("p")}`;
  const post = await upload(request, uploader, caption);
  const work = `My photo "Sunset over the Elbe" ${uniqueName("w")}`;

  // The claimant has no account.
  const claimantContext = await browser.newContext({ baseURL });
  const claimant = await claimantContext.newPage();
  await claimant.goto("/rights");
  await claimant.getByLabel("Urheberrecht").check();
  await claimant.getByLabel("Link zum Beitrag auf Klar").fill(`${baseURL}/posts/${post}`);
  await claimant.getByLabel("Welches Werk wird verletzt?").fill(work);
  await claimant.getByLabel("Warum stehen dir die Rechte zu?").fill("I took it myself on 3 May 2026.");
  await claimant.getByLabel("Dein Name").fill("Erika Mustermann");
  await claimant.getByLabel("Deine E-Mail-Adresse").fill(`${uniqueName("erika")}@example.test`);
  const send = claimant.getByRole("button", { name: "Meldung absenden" });
  await expect(send).toBeDisabled();
  await claimant.getByLabel(/in gutem Glauben/).check();
  await send.click();
  await expect(claimant.getByText("Deine Meldung ist eingegangen.")).toBeVisible();
  await claimant.getByRole("link", { name: "Zum Stand deiner Meldung" }).click();
  await expect(claimant.getByText("Eingegangen", { exact: true })).toBeVisible();

  // The team takes it on and asks for evidence.
  const adminPage = await pageFor(browser, admin);
  await adminPage.goto("/admin/rights");
  const card = adminPage.locator("div.rounded-xl").filter({ hasText: work });
  await expect(card).toContainText(caption);
  await card.getByRole("button", { name: "Take on" }).click();
  await expect(card).toContainText("In review");
  await card.getByPlaceholder(/Message to the claimant/).fill("Please send us the original file's metadata.");
  await card.getByRole("button", { name: "Ask for evidence" }).click();
  await expect(card).toContainText("Waiting for claimant");

  // The claimant answers on the status page.
  await claimant.reload();
  await expect(claimant.getByText("Rückfrage", { exact: true })).toBeVisible();
  await expect(claimant.getByText("Please send us the original file's metadata.")).toBeVisible();
  await claimant.getByPlaceholder("Deine Antwort").fill("Camera: X100V, taken 3 May 2026, 20:41.");
  await claimant.getByRole("button", { name: "Antwort senden" }).click();
  await expect(claimant.getByText("In Prüfung", { exact: true })).toBeVisible();

  // Accepted: the post is hidden for everyone else.
  await adminPage.reload();
  await expect(card).toContainText("Camera: X100V");
  await card.getByRole("button", { name: "Accept & hide post" }).click();
  await expect(card).toBeHidden();
  const other = await pageFor(browser, bystander);
  await other.goto(`/posts/${post}`);
  await expect(other.getByText("This post isn't available")).toBeVisible();
  await claimant.reload();
  await expect(claimant.getByText("Angenommen", { exact: true })).toBeVisible();

  // The uploader gets a statement naming the work, and objects.
  const own = await pageFor(browser, uploader);
  await own.goto("/moderation");
  await own.getByRole("link", { name: /Hidden · post/ }).click();
  await expect(own.getByText(/§ 97 UrhG/)).toBeVisible();
  await expect(own.getByText(`Betroffenes Werk laut Meldung: ${work}`)).toBeVisible();
  const objection = `I licensed this photo from the author ${uniqueName("o")}`;
  await own.getByPlaceholder("Warum ist die Entscheidung aus deiner Sicht falsch?").fill(objection);
  await own.getByRole("button", { name: "Widerspruch senden" }).click();

  await adminPage.goto("/admin/moderation");
  const objectionCard = adminPage.locator("div.rounded-xl").filter({ hasText: objection });
  await objectionCard.getByPlaceholder("Response to the user (required)").fill("The licence checks out; the post is back.");
  await objectionCard.getByRole("button", { name: "Accept" }).click();
  await expect(objectionCard).toBeHidden();

  // Visible again, and the claimant learns it was restored.
  await other.reload();
  await expect(other.getByText(caption)).toBeVisible();
  await claimant.reload();
  await expect(claimant.getByText("Wiederhergestellt", { exact: true })).toBeVisible();
  await claimantContext.close();
});

test("the public form refuses links that aren't a Klar post", async ({ browser, baseURL }) => {
  const context = await browser.newContext({ baseURL });
  const page = await context.newPage();
  await page.goto("/rights");
  await page.getByLabel("Link zum Beitrag auf Klar").fill("https://example.com/some/photo.jpg");
  await page.getByLabel("Welches Werk wird verletzt?").fill("A photo");
  await page.getByLabel("Warum stehen dir die Rechte zu?").fill("I took it.");
  await page.getByLabel("Dein Name").fill("Max");
  await page.getByLabel("Deine E-Mail-Adresse").fill(`${uniqueName("max")}@example.test`);
  await page.getByLabel(/in gutem Glauben/).check();
  await page.getByRole("button", { name: "Meldung absenden" }).click();
  await expect(page.getByText("The link must point to a post on Klar (…/posts/…)")).toBeVisible();
  await context.close();
});
