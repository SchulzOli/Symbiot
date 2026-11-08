import { defineConfig } from "@playwright/test";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

// Needs the UI built first: `cd apps/ui && trunk build --release`.
const root = resolve(__dirname, "..");
const llmPort = 8102;

// One server per spec file, each with fresh data: the tests count nodes and chain entries,
// so they must never see each other's chains.
function server(port: number, env: Record<string, string> = {}) {
  return {
    command: "cargo run --release -p symbiot-server",
    cwd: root,
    url: `http://127.0.0.1:${port}/health`,
    timeout: 300_000,
    reuseExistingServer: false,
    env: {
      SYMBIOT_ADDR: `127.0.0.1:${port}`,
      SYMBIOT_DATA: mkdtempSync(join(tmpdir(), "symbiot-e2e-")),
      SYMBIOT_UI: join(root, "apps", "ui", "dist"),
      RUST_LOG: "warn",
      ...env,
    },
  };
}

// #30, #31: a server whose store already holds raw data, loaded with the import tool before it
// starts. Each import is `[source, fixture file, extra arguments]`.
function serverWithImports(port: number, imports: [string, string, string?][]) {
  const base = server(port);
  const data = base.env.SYMBIOT_DATA;
  const fixtures = join(root, "e2e", "fixtures");
  const imp = ([source, file, extra = ""]: [string, string, string?]) =>
    `cargo run --release -q -p symbiot-import -- ${source} "${join(fixtures, file)}" --daten "${data}" ${extra}`;
  return { ...base, command: [...imports.map(imp), base.command].join(" && ") };
}

export default defineConfig({
  testDir: "tests",
  timeout: 30_000,
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? [["github"], ["html", { open: "never" }]] : "list",
  use: { trace: "retain-on-failure" },
  projects: [
    {
      name: "verknuepfen",
      testMatch: "verknuepfen.spec.ts",
      use: { baseURL: "http://127.0.0.1:8101" },
    },
    {
      name: "mitdenken",
      testMatch: "mitdenken.spec.ts",
      use: { baseURL: "http://127.0.0.1:8103" },
    },
    {
      name: "sensibel",
      testMatch: "sensibel.spec.ts",
      use: { baseURL: "http://127.0.0.1:8107" },
    },
    {
      name: "zeitleiste",
      testMatch: "zeitleiste.spec.ts",
      use: { baseURL: "http://127.0.0.1:8109" },
    },
    {
      name: "personen",
      testMatch: "personen.spec.ts",
      use: { baseURL: "http://127.0.0.1:8111" },
    },
    {
      name: "canvas",
      testMatch: "canvas.spec.ts",
      use: { baseURL: "http://127.0.0.1:8113" },
    },
    {
      name: "assistent",
      testMatch: "assistent.spec.ts",
      use: { baseURL: "http://127.0.0.1:8115" },
    },
    {
      name: "quellen",
      testMatch: "quellen.spec.ts",
      grep: /widerrufen/,
      use: { baseURL: "http://127.0.0.1:8117" },
    },
    {
      name: "importieren",
      testMatch: "quellen.spec.ts",
      grep: /Export in der App/,
      use: { baseURL: "http://127.0.0.1:8119" },
    },
    {
      name: "geraete",
      testMatch: "geraete.spec.ts",
      use: { baseURL: "http://127.0.0.1:8105" },
    },
  ],
  webServer: [
    server(8101),
    {
      // Stand-in for LiteLLM, so „Nächsten Schritt vorschlagen“ is tested without a real model.
      command: "node mock-llm.mjs",
      url: `http://127.0.0.1:${llmPort}/health`,
      reuseExistingServer: false,
      env: { MOCK_LLM_PORT: String(llmPort) },
    },
    server(8103, {
      SYMBIOT_AI_URL: `http://127.0.0.1:${llmPort}/v1`,
      SYMBIOT_AI_MODEL: "mock",
    }),
    server(8105),
    server(8113),
    server(8119),
    server(8115, {
      SYMBIOT_AI_URL: `http://127.0.0.1:${llmPort}/v1`,
      SYMBIOT_AI_MODEL: "mock",
    }),
    serverWithImports(8109, [
      ["whatsapp", "whatsapp-wandern.txt", "--chat Wandergruppe --utc-offset 60"],
      ["mbox", "rechnung.mbox"],
    ]),
    serverWithImports(8111, [
      ["whatsapp", "whatsapp-wandern.txt", "--chat Wandergruppe --utc-offset 60"],
      ["mbox", "anna.mbox"],
    ]),
    serverWithImports(8117, [
      ["whatsapp", "whatsapp-wandern.txt", "--chat Wandergruppe --utc-offset 60"],
      ["mbox", "rechnung.mbox"],
    ]),
    server(8107, {
      SYMBIOT_AI_URL: `http://127.0.0.1:${llmPort}/v1`,
      SYMBIOT_AI_MODEL: "mock",
    }),
  ],
});
