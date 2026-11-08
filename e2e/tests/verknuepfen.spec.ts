import { expect, test } from "@playwright/test";

// F1 + F2 + F3 + E2 in one walk through the real UI: capture, connect, edit, see the graph,
// verify the chain, unlink and delete.
test("erfassen, verbinden, bearbeiten, Graph, Kette prüfen, löschen", async ({ page }) => {
  await page.goto("/");
  const capture = page.locator("form.capture");
  const cards = page.getByTestId("canvas-node");

  async function festhalten(kind: string, title: string) {
    await capture.locator("select").selectOption(kind);
    await capture.getByPlaceholder("Was beschäftigt dich?").fill(title);
    await capture.getByRole("button", { name: "Festhalten" }).click();
    await expect(cards.filter({ hasText: title })).toHaveCount(1);
  }
  await festhalten("goal", "Laufen");
  await festhalten("evidence", "5 km gelaufen");
  await expect(cards).toHaveCount(2);

  // Connect: evidence supports goal.
  await cards.filter({ hasText: "5 km gelaufen" }).click();
  const detail = page.getByTestId("detail");
  await detail.getByLabel("Art der Verbindung").selectOption("supports");
  await detail.getByLabel("Ziel der Verbindung").selectOption({ label: "Ziel: Laufen" });
  await detail.getByRole("button", { name: "Verbinden" }).click();
  await expect(detail.getByText("→ stützt „Laufen“")).toBeVisible();

  // The same connection twice is refused.
  await detail.getByRole("button", { name: "Verbinden" }).click();
  await expect(detail.getByText("Diese Verbindung gibt es schon")).toBeVisible();

  // Editing appends a version; the old one stays in the history.
  await detail.getByLabel("Titel").fill("5,2 km gelaufen");
  await detail.getByRole("button", { name: "Neue Version speichern" }).click();
  await expect(detail.locator(".history li")).toHaveCount(2);
  await expect(detail.locator(".history li").first()).toContainText("„5 km gelaufen“");
  await expect(detail.locator(".history li").last()).toContainText("„5,2 km gelaufen“");

  // Graph: two nodes, one "supports" arrow.
  // E15: the canvas shows the connection as an arrow between the two cards.
  await expect(cards).toHaveCount(2);
  const edge = page.getByTestId("canvas-edge");
  await expect(edge).toHaveCount(1);
  await expect(edge).toHaveAttribute("data-kind", "supports");

  // 2 nodes + 1 link + 1 edit = 4 signed entries, chain intact.
  await page.getByRole("button", { name: "Kette prüfen" }).click();
  await expect(page.getByText(/✓ Kette ok · 4 Einträge/)).toBeVisible();

  // Unlink, then delete the evidence: both versions lose their content, the chain stays valid.
  // The list shows the same nodes; Enter opens one like a click.
  await page.getByRole("button", { name: "Liste" }).click();
  await page.getByTestId("node-card").filter({ hasText: "5,2 km gelaufen" }).focus();
  await page.keyboard.press("Enter");
  await detail.getByRole("button", { name: "Verbindung lösen" }).click();
  await expect(detail.getByText("→ stützt „Laufen“")).toHaveCount(0);
  await detail.getByRole("button", { name: "Löschen" }).click();
  await expect(page.getByTestId("confirm-delete")).toContainText("Titel und Text aller Versionen");
  await page.getByRole("button", { name: "Endgültig löschen" }).click();
  await expect(page.getByTestId("node-card")).toHaveCount(1);
  await expect(page.getByTestId("detail")).toHaveCount(0);

  // + 1 unlink + 1 deletion marker = 6 entries, 2 of them without content.
  await page.getByRole("button", { name: "Kette prüfen" }).click();
  await expect(page.getByText("✓ Kette ok · 6 Einträge, davon 2 ohne Inhalt (gelöscht)")).toBeVisible();
});
