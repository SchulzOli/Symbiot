import { defineConfig } from "@playwright/test";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

// Needs the UI built first: `cd apps/ui && trunk build --release`.
const root = resolve(__dirname, "..");
const port = 8101;
const llmPort = 8102;

export default defineConfig({
  testDir: "tests",
  timeout: 30_000,
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? [["github"], ["html", { open: "never" }]] : "list",
  use: { baseURL: `http://127.0.0.1:${port}`, trace: "retain-on-failure" },
  webServer: [
    {
      // Stand-in for LiteLLM, so „Nächsten Schritt vorschlagen“ is tested without a real model.
      command: "node mock-llm.mjs",
      url: `http://127.0.0.1:${llmPort}/health`,
      reuseExistingServer: false,
      env: { MOCK_LLM_PORT: String(llmPort) },
    },
    {
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
      SYMBIOT_AI_URL: `http://127.0.0.1:${llmPort}/v1`,
      SYMBIOT_AI_MODEL: "mock",
    },
    },
  ],
});
