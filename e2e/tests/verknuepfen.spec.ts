import { expect, test } from "@playwright/test";

// F1 + F2 + F3 in one walk through the real UI: capture, connect, edit, see the graph, verify the chain.
test("erfassen, verbinden, bearbeiten, Graph, Kette prüfen", async ({ page }) => {
  await page.goto("/");
  const capture = page.locator("form.capture");
  const cards = page.getByTestId("node-card");

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
  await page.getByRole("button", { name: "Graph" }).click();
  await expect(page.getByTestId("graph-node")).toHaveCount(2);
  const edge = page.getByTestId("graph-edge");
  await expect(edge).toHaveCount(1);
  await expect(edge).toHaveAttribute("data-kind", "supports");

  // 2 nodes + 1 link + 1 edit = 4 signed entries, chain intact.
  await page.getByRole("button", { name: "Kette prüfen" }).click();
  await expect(page.getByText(/✓ Kette ok · 4 Einträge/)).toBeVisible();
});
