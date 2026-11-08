import { expect, test } from "@playwright/test";

// #31: the same person in WhatsApp and in a mail. Symbiot suggests, the human merges; the
// person's timeline shows both sources; splitting and „not the same“ undo and prevent it.
test("Personen: Vorschlag annehmen, Zeitleiste der Person, trennen", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Personen", exact: true }).click();
  const persons = page.getByTestId("persons");
  const cards = persons.getByTestId("person");
  await expect(cards).toHaveCount(4); // Anna, Ben (WhatsApp), Anna Beispiel, me (mail)

  const suggestion = persons.getByTestId("person-suggestion");
  await expect(suggestion).toHaveCount(1);
  await expect(suggestion).toContainText("Anna");
  await expect(suggestion).toContainText("Anna Beispiel");
  await expect(suggestion).toContainText("gleicher Vorname");
  await suggestion.getByRole("button", { name: "Zusammenführen" }).click();

  await expect(cards).toHaveCount(3);
  const anna = cards.filter({ hasText: "Anna Beispiel" });
  await expect(anna).toContainText("name:Anna");
  await expect(anna).toContainText("mailto:anna@example.org");
  await expect(anna).toContainText("3 Belege");
  await expect(suggestion).toHaveCount(0);

  // „Was habe ich mit wem gemacht“: both sources in one timeline.
  await anna.getByRole("button", { name: "Zeitleiste zeigen" }).click();
  const timeline = page.getByTestId("timeline");
  await expect(timeline.getByTestId("person-filter")).toHaveText("Person: Anna Beispiel");
  const items = timeline.getByTestId("evidence");
  await expect(items).toHaveCount(3);
  await expect(items.first()).toContainText("Fotos vom See");
  await timeline.getByRole("button", { name: "alle zeigen" }).click();
  await expect(items).toHaveCount(5);

  // Undo: split the WhatsApp name off again, then say they are not the same.
  await page.getByRole("button", { name: "Personen", exact: true }).click();
  await anna.getByRole("button", { name: "name:Anna trennen" }).click();
  await expect(cards).toHaveCount(4);
  await expect(suggestion).toHaveCount(1);
  await suggestion.getByRole("button", { name: "Nicht dieselbe" }).click();
  await expect(suggestion).toHaveCount(0);
  await expect(cards).toHaveCount(4);

  // Persons are not the graph.
  await page.getByRole("button", { name: "Liste" }).click();
  await expect(page.getByText("Noch nichts festgehalten.")).toBeVisible();
});
