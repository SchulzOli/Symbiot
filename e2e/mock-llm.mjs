// Stand-in for LiteLLM in the E2E test: an OpenAI-compatible /v1/chat/completions that
// proposes „Laufschuhe kaufen“ and cites the first node id it finds in the prompt (the goal),
// suggests the rule „Gesundheit“ when asked for rules (E4), or a step with a link when
// asked to do something (E16).
import { createServer } from "node:http";

const port = Number(process.env.MOCK_LLM_PORT ?? 8102);

createServer((req, res) => {
  if (req.method === "GET" && req.url === "/health") {
    res.end("ok");
    return;
  }
  if (req.method !== "POST" || !req.url?.endsWith("/chat/completions")) {
    res.writeHead(404).end();
    return;
  }
  let body = "";
  req.on("data", (c) => (body += c));
  req.on("end", () => {
    const { model, messages } = JSON.parse(body);
    const prompt = messages.map((m) => m.content).join("\n");
    const ids = prompt.match(/\b[0-9a-f]{32}\b/g) ?? [];
    // E16: a request in words gets a step for the first node and a link to it.
    const assist = prompt.includes("Auftrag:")
      ? JSON.stringify({
          title: "Ersten Schritt anlegen",
          reason: "Ein kleiner Schritt macht das Ziel greifbar.",
          actions: [
            { create_node: { kind: "goal", title: "Laufschuhe kaufen" } },
            { link: { from: "$0", to: ids[0], kind: "derives_from" } },
          ],
        })
      : null;
    // E4: a request for rules gets „Gesundheit“; Symbiot checks what it matches itself.
    const content = assist ?? (prompt.includes("Regeln für sensible Inhalte")
      ? JSON.stringify({ rules: [{ label: "Gesundheit", keywords: ["arzt", "befund"] }] })
      : JSON.stringify({
          title: "Laufschuhe kaufen",
          body: "Die alten Schuhe sind durch.",
          sources: ids.slice(0, 2),
        }));
    res.writeHead(200, { "content-type": "application/json" });
    res.end(JSON.stringify({ model, choices: [{ message: { role: "assistant", content } }] }));
  });
}).listen(port, "127.0.0.1");
