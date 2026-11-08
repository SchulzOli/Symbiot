import { expect, test } from "@playwright/test";

// E4: the AI suggests a rule for sensitive content, the human accepts it, matching nodes get the lock.
test("KI schlägt Regel für Sensibles vor, du nimmst sie an", async ({ page }) => {
  await page.goto("/");
  const capture = page.locator("form.capture");
  const cards = page.getByTestId("canvas-node");
  async function festhalten(kind: string, title: string) {
    await capture.locator("select").selectOption(kind);
    await capture.getByPlaceholder("Was beschäftigt dich?").fill(title);
    await capture.getByRole("button", { name: "Festhalten" }).click();
    await expect(cards.filter({ hasText: title })).toHaveCount(1);
  }
  await festhalten("reflection", "Befund vom Arzt");
  await festhalten("goal", "Laufen");

  const openRules = () => page.getByRole("button", { name: "Regeln für Sensibles" }).click();
  await openRules();
  const rules = page.getByTestId("rules");
  await expect(rules.getByText("Noch keine Regel.")).toBeVisible();
  await rules.getByRole("button", { name: "KI nach Regeln fragen" }).click();
  const proposal = rules.getByTestId("rule-proposal");
  await expect(proposal).toContainText("Gesundheit");
  await expect(proposal).toContainText("Würde 1 Knoten markieren: Befund vom Arzt");
  // Nothing is sensitive until the human accepts.
  await expect(cards.filter({ hasText: "Befund vom Arzt" }).locator(".lock-mark")).toHaveCount(0);

  await proposal.getByRole("button", { name: "Annehmen" }).click();
  await expect(rules.locator(".rule-list li")).toContainText("Gesundheit: arzt, befund · vorgeschlagen von mock");
  await expect(cards.filter({ hasText: "Befund vom Arzt" }).locator(".lock-mark")).toHaveCount(1);
  await expect(cards.filter({ hasText: "Laufen" }).locator(".lock-mark")).toHaveCount(0);

  await page.keyboard.press("Escape"); // close the rules panel
  await cards.filter({ hasText: "Befund vom Arzt" }).click();
  await expect(page.getByTestId("detail")).toContainText("(durch Regel „Gesundheit“)");

  // A rule of one's own. Opening a node closed the rules panel; open it again.
  await openRules();
  await rules.getByLabel("Name der Regel").fill("Finanzen");
  await rules.getByLabel("Stichwörter").fill("Konto, Steuer");
  await rules.getByRole("button", { name: "Regel hinzufügen" }).click();
  await expect(rules.locator(".rule-list li")).toHaveCount(2);
  await expect(rules.locator(".rule-list li").last()).toContainText("Finanzen: konto, steuer");
});
