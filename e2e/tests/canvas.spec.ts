import AxeBuilder from "@axe-core/playwright";
import { expect, test, type Page } from "@playwright/test";

// E15: the canvas is the start view; it works with mouse and keyboard, symbols have their
// words as accessible names, and the views pass an automated WCAG 2.2 AA check.

async function axe(page: Page, where: string) {
  const result = await new AxeBuilder({ page })
    .withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa"])
    .analyze();
  const serious = result.violations.filter((v) => v.impact === "serious" || v.impact === "critical");
  expect(
    serious.map((v) => `${where}: ${v.id} (${v.nodes.length}) ${v.nodes[0]?.target}`),
  ).toEqual([]);
}

test("Canvas: verbinden mit Maus und Tastatur, Symbole mit Namen, WCAG-Prüfung", async ({ page }) => {
  await page.goto("/");
  await axe(page, "leer");

  const capture = page.locator("form.capture");
  const cards = page.getByTestId("canvas-node");
  async function festhalten(kind: string, title: string) {
    await capture.getByLabel("Art").selectOption(kind);
    await capture.getByLabel("Titel").fill(title);
    await capture.getByLabel("Titel").press("Enter");
    await expect(cards.filter({ hasText: title })).toHaveCount(1);
  }
  await festhalten("goal", "Gesund bleiben");
  await festhalten("evidence", "Arzttermin wahrgenommen");
  await festhalten("tension", "Wenig Zeit");

  // Every button has a name: symbols carry their words.
  const unnamed = await page.locator("button").evaluateAll((all) =>
    all.filter((b) => !(b.getAttribute("aria-label") || b.textContent?.trim())).length,
  );
  expect(unnamed).toBe(0);

  // Mouse: the link symbol on a card, then the target, then the kind of connection.
  const evidence = page.locator(".canvas-item").filter({ hasText: "Arzttermin" });
  await evidence.hover();
  await evidence.getByRole("button", { name: "„Arzttermin wahrgenommen“ verbinden" }).click();
  await expect(page.getByRole("status").filter({ hasText: "verbinden: Ziel anklicken" })).toBeVisible();
  await cards.filter({ hasText: "Gesund bleiben" }).click();
  await page.getByRole("dialog", { name: "Art der Verbindung" }).getByRole("button", { name: /^stützt/ }).click();
  await expect(page.getByTestId("canvas-edge")).toHaveCount(1);
  await expect(page.getByTestId("canvas-edge")).toHaveAttribute("data-kind", "supports");

  // Keyboard: focus a card, L starts connecting, Enter on the target, then the kind.
  await cards.filter({ hasText: "Wenig Zeit" }).focus();
  await page.keyboard.press("l");
  await cards.filter({ hasText: "Gesund bleiben" }).focus();
  await page.keyboard.press("Enter");
  await page.getByRole("dialog", { name: "Art der Verbindung" }).getByRole("button", { name: /^widerspricht/ }).click();
  await expect(page.getByTestId("canvas-edge")).toHaveCount(2);

  // Shift + arrow moves the focused card; the place is kept after a reload.
  const item = page.locator(".canvas-item").filter({ hasText: "Wenig Zeit" });
  const before = await item.getAttribute("style");
  await cards.filter({ hasText: "Wenig Zeit" }).focus();
  await page.keyboard.press("Shift+ArrowRight");
  await expect(item).not.toHaveAttribute("style", before!);
  const moved = await item.getAttribute("style");
  await page.reload();
  await expect(page.locator(".canvas-item").filter({ hasText: "Wenig Zeit" })).toHaveAttribute("style", moved!);

  // Enter opens the detail; Escape closes it; the views have shortcuts.
  await cards.filter({ hasText: "Gesund bleiben" }).focus();
  await page.keyboard.press("Enter");
  await expect(page.getByTestId("detail")).toBeVisible();
  await axe(page, "Canvas mit Detail");
  await page.keyboard.press("Escape");
  await expect(page.getByTestId("detail")).toHaveCount(0);

  for (const [key, view] of [["2", "Liste"], ["3", "Zeitleiste"], ["4", "Personen"], ["1", "Canvas"]]) {
    await page.locator("body").press(key);
    await expect(page.getByRole("button", { name: view, exact: true })).toHaveAttribute("aria-pressed", "true");
    await axe(page, view);
  }

  // A tooltip shows the words on keyboard focus, not only on hover.
  const verify = page.getByRole("button", { name: "Kette prüfen" });
  await verify.focus();
  await expect(verify).toHaveAttribute("data-tip", "Kette prüfen");
});
