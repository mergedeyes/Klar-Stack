import { expect, test } from "@playwright/test";
import { adminSession, pageFor, signUp, uniqueName, upload } from "./helpers";

// A notice about illegal content from someone without an account, through
// the public form: the queue shows it as a public notice, and the notifier
// follows it on the status page through to the decision.

test("anyone can report illegal content through the public form and follow it", async ({ browser, request, baseURL }) => {
  const admin = await adminSession();
  const author = await signUp("author");
  const caption = `hateful ${uniqueName("p")}`;
  const post = await upload(request, author, caption);

  // The notifier has no account.
  const visitorContext = await browser.newContext({ baseURL });
  const visitor = await visitorContext.newPage();
  await visitor.goto("/");
  await visitor.getByRole("link", { name: "Rechtswidrige Inhalte melden" }).click();
  await expect(visitor).toHaveURL(/\/notices$/);
  await visitor.getByLabel("Volksverhetzung oder Hassrede").check();
  await visitor.getByLabel("Link zum Inhalt auf Klar").fill(`${baseURL}/posts/${post}`);
  await visitor.getByLabel("Warum ist der Inhalt rechtswidrig?").fill("Calls for violence against a religious group.");
  const send = visitor.getByRole("button", { name: "Meldung absenden" });
  await expect(send).toBeDisabled();
  await visitor.getByLabel("Dein Name").fill("Erika Muster");
  await visitor.getByLabel("Deine E-Mail-Adresse").fill(`${uniqueName("erika")}@example.test`);
  await visitor.getByLabel(/in gutem Glauben/).check();
  await send.click();
  await expect(visitor.getByText("Deine Meldung ist eingegangen.")).toBeVisible();
  await visitor.getByRole("link", { name: "Zum Stand deiner Meldung" }).click();
  await expect(visitor.getByText("In Prüfung", { exact: true })).toBeVisible();

  // In the queue as a public notice, with who sent it.
  const adminPage = await pageFor(browser, admin);
  await adminPage.goto("/admin/reports");
  const card = adminPage.getByTestId("report-group").filter({ hasText: caption });
  await expect(card).toContainText("Public notice from Erika Muster");
  await card.getByRole("button", { name: "Remove content" }).click();
  await expect(card).toBeHidden();

  // The decision reaches the status page.
  await visitor.reload();
  await expect(visitor.getByText("Wir haben den gemeldeten Inhalt geprüft und entfernt.")).toBeVisible();
  await visitorContext.close();
});

test("a notice about child sexual abuse material needs no name", async ({ browser, request, baseURL }) => {
  const author = await signUp("author");
  const post = await upload(request, author, `csam ${uniqueName("p")}`);
  const visitorContext = await browser.newContext({ baseURL });
  const visitor = await visitorContext.newPage();
  await visitor.goto("/notices");
  await visitor.getByLabel("Darstellung sexuellen Missbrauchs von Kindern").check();
  // What not to do, before anything else.
  await expect(visitor.getByText(/nicht herunter, mache keine Screenshots/)).toBeVisible();
  await visitor.getByLabel("Link zum Inhalt auf Klar").fill(`${baseURL}/posts/${post}`);
  await visitor.getByLabel("Warum ist der Inhalt rechtswidrig?").fill("Shows a child.");
  await visitor.getByLabel(/in gutem Glauben/).check();
  await visitor.getByRole("button", { name: "Meldung absenden" }).click();
  await expect(visitor.getByText(/keine E-Mail-Adresse angegeben/)).toBeVisible();
  await visitorContext.close();
});
