import { defineConfig } from "@playwright/test";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

// Needs the UI built first: `cd apps/ui && trunk build --release`.
const root = resolve(__dirname, "..");
const port = 8101;

export default defineConfig({
  testDir: "tests",
  timeout: 30_000,
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? [["github"], ["html", { open: "never" }]] : "list",
  use: { baseURL: `http://127.0.0.1:${port}`, trace: "retain-on-failure" },
  webServer: {
    command: "cargo run --release -p symbiot-server",
    cwd: root,
    url: `http://127.0.0.1:${port}/health`,
    timeout: 300_000,
    reuseExistingServer: false,
    env: {
      SYMBIOT_ADDR: `127.0.0.1:${port}`,
      // Fresh data every run: tests never see each other's chains.
      SYMBIOT_DATA: mkdtempSync(join(tmpdir(), "symbiot-e2e-")),
      SYMBIOT_UI: join(root, "apps", "ui", "dist"),
      RUST_LOG: "warn",
    },
  },
});
