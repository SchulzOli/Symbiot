import { expect, test } from "@playwright/test";

// E16: you say in words what should happen; the AI suggests actions — the same ones you can
// do yourself. Nothing changes until you accept; single actions can be left out, and what
// builds on a left-out new node is left out with it.
test("Assistent: Vorschlag aus Aktionen, auswählen, ausführen, ablehnen", async ({ page }) => {
  await page.goto("/");
  const cards = page.getByTestId("canvas-node");
  const verify = async (entries: number) => {
    await page.getByRole("button", { name: "Kette prüfen" }).click();
    await expect(page.getByText(new RegExp(`✓ Kette ok · ${entries} Eintr`))).toBeVisible();
  };
  const capture = page.locator("form.capture");
  await capture.getByLabel("Titel").fill("Laufen");
  await capture.getByLabel("Titel").press("Enter");
  await expect(cards).toHaveCount(1);

  // The shortcut A (outside a text field) opens the assistant.
  await page.getByRole("heading", { name: "Symbiot" }).click();
  await page.keyboard.press("a");
  const assistant = page.getByTestId("assistant");
  await assistant.getByLabel("Was soll geschehen?").fill("Leg für Laufen einen ersten Schritt an");
  await assistant.getByRole("button", { name: "Vorschlag holen" }).click();

  const suggestion = assistant.getByTestId("suggestion");
  await expect(suggestion.getByText("Ersten Schritt anlegen")).toBeVisible();
  const step = suggestion.getByLabel("Neu: Ziel „Laufschuhe kaufen“");
  const link = suggestion.getByLabel("Verbinden: „Laufschuhe kaufen“ (neu) ist abgeleitet von „Laufen“");
  await expect(step).toBeChecked();
  await expect(link).toBeChecked();
  await expect(cards).toHaveCount(1); // nothing changed yet
  await verify(1);

  // Leaving out the new node leaves out the link to it; choosing the link brings it back.
  await step.uncheck();
  await expect(link).not.toBeChecked();
  await link.check();
  await expect(step).toBeChecked();

  await suggestion.getByRole("button", { name: "Ausgewählte ausführen" }).click();
  await expect(assistant.getByText("2 Aktionen ausgeführt.")).toBeVisible();
  await expect(cards).toHaveCount(2);
  await expect(page.getByTestId("canvas-edge")).toHaveAttribute("data-kind", "derives_from");
  await verify(4); // goal, new node, link, decision

  // Rejecting changes nothing but is kept as a decision.
  await assistant.getByLabel("Was soll geschehen?").fill("Noch ein Schritt");
  await assistant.getByRole("button", { name: "Vorschlag holen" }).click();
  await assistant.getByTestId("suggestion").getByRole("button", { name: "Ablehnen" }).click();
  await expect(assistant.getByText(/Abgelehnt\. Nichts wurde geändert/)).toBeVisible();
  await expect(cards).toHaveCount(2);
  await verify(5);
});
