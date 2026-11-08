import AxeBuilder from "@axe-core/playwright";
import { expect, test, type Page } from "@playwright/test";

// #44: which raw data is used. Pause a source, leave out a chat, revoke a source — each a
// signed decision; the timeline and the persons follow at once.

async function axe(page: Page, where: string) {
  const result = await new AxeBuilder({ page })
    .withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa"])
    .analyze();
  const serious = result.violations.filter((v) => v.impact === "serious" || v.impact === "critical");
  expect(
    serious.map((v) => `${where}: ${v.id} (${v.nodes.length}) ${v.nodes[0]?.target}`),
  ).toEqual([]);
}

test("Quellen: pausieren, Chat auslassen, widerrufen", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("heading", { name: "Symbiot" }).click();
  await page.keyboard.press("5");
  const view = page.getByTestId("sources");
  const sources = view.getByTestId("source");
  await expect(sources).toHaveCount(2);
  const whatsapp = sources.filter({ has: page.getByRole("heading", { name: "whatsapp", exact: true }) });
  const mail = sources.filter({ has: page.getByRole("heading", { name: "mail", exact: true }) });
  await expect(whatsapp).toContainText("aktiv");
  await expect(whatsapp.getByTestId("source-used")).toHaveText("4 Belege genutzt");
  await axe(page, "Quellen");

  // Pause: the data stays, but the timeline no longer shows it.
  await whatsapp.getByRole("button", { name: "whatsapp pausieren" }).click();
  await expect(whatsapp).toContainText("pausiert");
  await expect(whatsapp.getByTestId("source-used")).toHaveText("0 von 4 Belegen genutzt");
  await page.getByRole("button", { name: "Zeitleiste", exact: true }).click();
  const timeline = page.getByTestId("timeline");
  await expect(timeline.getByTestId("evidence")).toHaveCount(1);
  await expect(timeline.getByTestId("evidence")).toContainText("mail");

  // Use it again, but leave the chat out.
  await page.getByRole("button", { name: "Quellen", exact: true }).click();
  await whatsapp.getByRole("button", { name: "whatsapp nutzen" }).click();
  await expect(whatsapp.getByTestId("source-used")).toHaveText("4 Belege genutzt");
  await whatsapp.getByRole("button", { name: "Auswahl" }).click();
  await whatsapp.getByRole("checkbox", { name: /Wandergruppe/ }).uncheck();
  await expect(whatsapp.getByTestId("source-used")).toHaveText("0 von 4 Belegen genutzt");
  await expect(whatsapp).toContainText("1 ausgelassen");
  await whatsapp.getByRole("button", { name: "Auswahl" }).click();
  await whatsapp.getByRole("checkbox", { name: /Wandergruppe/ }).check();
  await expect(whatsapp.getByTestId("source-used")).toHaveText("4 Belege genutzt");

  // Revoke: asks first, then the raw data is gone.
  await mail.getByRole("button", { name: "mail widerrufen" }).click();
  await expect(mail.getByRole("alertdialog")).toContainText("Alle Rohdaten von mail (1 Beleg)");
  await axe(page, "Widerrufen bestätigen");
  await mail.getByRole("button", { name: "Endgültig löschen" }).click();
  await expect(mail).toContainText("widerrufen");
  await expect(mail).toContainText("0 Belege");
  await page.getByRole("button", { name: "Zeitleiste", exact: true }).click();
  await expect(timeline.getByTestId("evidence")).toHaveCount(4);

  // The chain still verifies: content removed, heads and signatures stay.
  await page.getByRole("button", { name: "Kette prüfen" }).click();
  await expect(page.getByRole("status").first()).toContainText("ohne Inhalt");
});
