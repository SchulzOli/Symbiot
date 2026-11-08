import { expect, test } from "@playwright/test";

// #30: imported raw data shows up as a timeline and can be searched and filtered.
test("Zeitleiste zeigt importierte Rohdaten, Suche und Herkunft filtern", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Zeitleiste" }).click();
  const timeline = page.getByTestId("timeline");
  const items = timeline.getByTestId("evidence");

  // Three chat messages and one mail, newest first, grouped by day.
  await expect(items).toHaveCount(4);
  await expect(items.first()).toContainText("Zum See.");
  await expect(items.first()).toContainText("Mit Picknick!");
  await expect(items.first()).toContainText("Anna");
  await expect(items.first()).toContainText("Wandergruppe");
  await expect(items.last()).toContainText("Buchhaltung");
  await expect(items.last()).toContainText("Rechnung März");
  await expect(timeline.locator(".day")).toHaveCount(2);

  await expect(timeline.getByRole("button", { name: "Alle (4)" })).toBeVisible();
  await timeline.getByRole("button", { name: "mail (1)" }).click();
  await expect(items).toHaveCount(1);
  await timeline.getByRole("button", { name: "Alle (4)" }).click();
  await expect(items).toHaveCount(4);

  const search = timeline.getByLabel("Rohdaten durchsuchen");
  await search.fill("samstag");
  await search.press("Enter");
  await expect(items).toHaveCount(1);
  await expect(items.first()).toContainText("Wandern am Samstag?");

  await search.fill("gibtsnicht");
  await search.press("Enter");
  await expect(timeline.getByText("Nichts gefunden.")).toBeVisible();

  // Raw data is not the graph.
  await page.getByRole("button", { name: "Liste" }).click();
  await expect(page.getByText("Noch nichts festgehalten.")).toBeVisible();
});
