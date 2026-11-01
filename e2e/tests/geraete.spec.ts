import { expect, test } from "@playwright/test";

// M4 / E5 in the web UI on sz: set up a new person key, check three words, confirm this device.
test("Personen-Schlüssel anlegen und Gerät bestätigen", async ({ page }) => {
  await page.goto("/");
  const devices = page.getByTestId("devices");
  await expect(devices.getByTestId("setup")).toBeVisible();

  await devices.getByRole("button", { name: "Personen-Schlüssel anlegen" }).click();
  const words = await devices.getByTestId("words").locator("li").allInnerTexts();
  expect(words).toHaveLength(24);

  // A wrong word is caught before anything is written.
  const checks = devices.getByLabel(/^Wort \d+$/);
  await expect(checks).toHaveCount(3);
  for (const input of await checks.all()) {
    await input.fill("falsch");
  }
  await devices.getByRole("button", { name: "Gerät bestätigen" }).click();
  await expect(devices.getByText(/Wort \d+ stimmt nicht/)).toBeVisible();

  for (const input of await checks.all()) {
    const n = Number((await input.getAttribute("aria-label"))!.replace("Wort ", ""));
    await input.fill(words[n - 1]);
  }
  await devices.getByRole("button", { name: "Gerät bestätigen" }).click();

  await expect(devices.locator(".device-list li")).toHaveCount(1);
  await expect(devices.locator(".device-list li")).toContainText("sz");
  await expect(devices.locator(".device-list li")).toContainText("dieses Gerät");
  await expect(devices.getByText("Dies ist sz: Hier gleichen deine Geräte ab.")).toBeVisible();
  // The words are gone from the page once confirmed.
  await expect(devices.getByTestId("words")).toHaveCount(0);

  // The certificate is one signed entry in this device's chain.
  await page.getByRole("button", { name: "Kette prüfen" }).click();
  await expect(page.getByText(/✓ Kette ok · 1 Eintrag/)).toBeVisible();
});
