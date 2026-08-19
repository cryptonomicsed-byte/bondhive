// MCP smoke test: boots the Bondhive MCP server on stdio, does initialize + tools/list.
// Waits on the process's actual "ready" line (stderr) and each JSON-RPC response
// (stdout) rather than fixed timers, so it doesn't flake under host load — a
// hardcoded-timeout version of this test previously reported false failures on a
// busy machine even though the server was working correctly.
const { spawn } = require("child_process");
const child = spawn("node", ["dist/index.js"], { stdio: ["pipe", "pipe", "pipe"] });
let out = "";
let ready = false;
const TIMEOUT_MS = 10000;

const send = (msg) => child.stdin.write(JSON.stringify(msg) + "\n");

const fail = (reason) => {
  console.log("RAW OUTPUT:\n" + out.slice(0, 2000));
  console.log("MCP SMOKE FAIL:", reason);
  child.kill();
  process.exit(1);
};

const overallTimeout = setTimeout(() => fail("timed out waiting for server"), TIMEOUT_MS);

child.stderr.on("data", (d) => {
  const s = d.toString();
  if (!ready && /ready/i.test(s)) {
    ready = true;
    send({ jsonrpc: "2.0", id: 1, method: "initialize", params: { protocolVersion: "2025-03-26", capabilities: {}, clientInfo: { name: "smoke", version: "0" } } });
  }
});

child.stdout.on("data", (d) => {
  out += d.toString();
  if (out.includes('"id":1') && !out.includes('"id":2')) {
    send({ jsonrpc: "2.0", id: 2, method: "tools/list", params: {} });
  }
  if (out.includes('"id":2')) {
    clearTimeout(overallTimeout);
    const names = [...out.matchAll(/"name":"([a-z0-9_.]+)"/g)].map((m) => m[1]);
    const uniq = [...new Set(names)];
    console.log("RAW OUTPUT:\n" + out.slice(0, 2000));
    console.log("TOOL NAMES:", uniq.join(", "));
    console.log("TOOL COUNT:", uniq.length);
    const ok = out.includes('"tools"') && uniq.length >= 10;
    console.log(ok ? "MCP SMOKE PASS" : "MCP SMOKE FAIL");
    child.kill();
    process.exit(ok ? 0 : 1);
  }
});

child.on("exit", (code) => {
  if (!out.includes('"id":2')) fail(`process exited early (code ${code})`);
});
