// A real stdio MCP service launched through npm, with marker-controlled work.
import { appendFileSync, existsSync, writeFileSync } from "node:fs";
import { createInterface } from "node:readline";
import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";

if (process.argv[2] === "--task") {
  const marker = process.argv[3];
  writeFileSync(`${marker}.started`, String(process.pid));
  const timer = setInterval(() => {
    if (existsSync(marker)) {
      writeFileSync(`${marker}.result`, "background result");
      clearInterval(timer);
    }
  }, 20);
} else {
  const marker = process.env.STALL_TEST_MARKER;
  const calls = process.env.STALL_TEST_CALLS;
  const tools = ["ping", "start_work"].map((name) => ({
    name,
    description: name === "ping" ? "Return a ping receipt" : "Start background work and return before it completes",
    inputSchema: { type: "object", properties: {}, additionalProperties: false },
  }));
  const lines = createInterface({ input: process.stdin });
  lines.on("line", (line) => {
    const request = JSON.parse(line);
    if (request.id === undefined) return;
    let result;
    switch (request.method) {
      case "initialize":
        if (process.env.STALL_TEST_INITIALIZED) appendFileSync(process.env.STALL_TEST_INITIALIZED, `${process.env.STALL_TEST_LABEL ?? "service"}\n`);
        result = { protocolVersion: request.params.protocolVersion, capabilities: { tools: {} }, serverInfo: { name: "late-service", version: "1.0" } };
        break;
      case "tools/list": result = { tools }; break;
      case "tools/call":
        if (calls) appendFileSync(calls, `${request.params.name}\n`);
        if (request.params.name === "start_work") {
          spawn(process.execPath, [fileURLToPath(import.meta.url), "--task", marker], { stdio: "ignore" });
        }
        result = { content: [{ type: "text", text: request.params.name === "ping" ? "ping receipt" : "background started" }] };
        break;
      default: result = {};
    }
    process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", id: request.id, result })}\n`);
  });
}
