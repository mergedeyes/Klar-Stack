import path from "node:path";
import { expect, test } from "@playwright/test";
import { signIn, signUp } from "./helpers";

// Feedback with screenshots: picked from the device, previewed, removable,
// at most three, sent along with the text.

const shot = (name: "landscape" | "portrait") => path.join(__dirname, "fixtures", `${name}.png`);

test("feedback can carry up to three screenshots", async ({ page, request }) => {
  await signIn(page, await signUp(request, "tester"));
  await page.goto("/feedback?from=/chats");

  await page.getByLabel("Your feedback").fill("The footer covers the send button");
  const files = page.getByLabel("Screenshot files");
  await files.setInputFiles([shot("portrait"), shot("landscape")]);
  await expect(page.getByAltText("Screenshot preview")).toHaveCount(2);

  await page.getByRole("button", { name: "Remove screenshot" }).first().click();
  await expect(page.getByAltText("Screenshot preview")).toHaveCount(1);

  await files.setInputFiles([shot("portrait"), shot("landscape"), shot("portrait")]);
  await expect(page.getByText("You can attach up to 3 screenshots.")).toBeVisible();
  await expect(page.getByAltText("Screenshot preview")).toHaveCount(3);
  await expect(page.getByRole("button", { name: "Add screenshot" })).toBeHidden();

  await page.getByRole("button", { name: "Send feedback" }).click();
  await expect(page.getByText("Thank you!")).toBeVisible();
});
