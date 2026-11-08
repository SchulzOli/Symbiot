import { expect, test } from "@playwright/test";

// F4 in the real UI: the AI proposes, nothing is written until the human decides.
test("KI schlägt vor, du entscheidest", async ({ page }) => {
  await page.goto("/");
  const capture = page.locator("form.capture");
  const cards = page.getByTestId("canvas-node");
  const verify = async (entries: number) => {
    await page.getByRole("button", { name: "Kette prüfen" }).click();
    await expect(page.getByText(new RegExp(`✓ Kette ok · ${entries} Einträge`))).toBeVisible();
  };

  async function festhalten(kind: string, title: string) {
    await capture.locator("select").selectOption(kind);
    await capture.getByPlaceholder("Was beschäftigt dich?").fill(title);
    await capture.getByRole("button", { name: "Festhalten" }).click();
    await expect(cards.filter({ hasText: title })).toHaveCount(1);
  }
  await festhalten("goal", "Laufen");
  await festhalten("evidence", "5 km gelaufen");

  await cards.filter({ hasText: "Laufen" }).filter({ hasNotText: "5 km" }).click();
  const detail = page.getByTestId("detail");

  // Proposing writes nothing: still 2 entries.
  await detail.getByRole("button", { name: "Nächsten Schritt vorschlagen" }).click();
  const proposal = detail.getByTestId("proposal");
  await expect(proposal.getByText("Laufschuhe kaufen")).toBeVisible();
  await expect(proposal.getByText("noch nicht gespeichert")).toBeVisible();
  await expect(proposal.locator(".sources li")).not.toHaveCount(0);
  await expect(cards).toHaveCount(2);
  await verify(2);

  // Rejecting records the decision, but adds no node.
  await proposal.getByRole("button", { name: "Ablehnen" }).click();
  await expect(proposal).toHaveCount(0);
  await expect(cards).toHaveCount(2);
  await verify(3);

  // Accepting creates the node, derived from the goal.
  await detail.getByRole("button", { name: "Nächsten Schritt vorschlagen" }).click();
  await detail.getByTestId("proposal").getByRole("button", { name: "Annehmen" }).click();
  await expect(cards.filter({ hasText: "Laufschuhe kaufen" })).toHaveCount(1);
  await expect(detail.getByText("→ ist abgeleitet von „Laufen“")).toBeVisible();
  await expect(detail.locator(".history li").first()).toContainText("KI-Vorschlag von mock, angenommen");
  await verify(4);

  // Marking by hand: one click, recorded in the chain, shown on the card.
  await detail.getByLabel("🔒 Sensibel – verlässt nie das Heimnetz").check();
  await expect(cards.filter({ hasText: "Laufschuhe kaufen" }).locator(".lock-mark")).toBeVisible();
  await verify(5);
});
