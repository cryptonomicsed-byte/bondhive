// MCP smoke test: boots the Bondhive MCP server on stdio, does initialize + tools/list
const { spawn } = require("child_process");
const child = spawn("node", ["dist/index.js"], { stdio: ["pipe", "pipe", "pipe"] });
let out = "";
let err = "";
child.stdout.on("data", (d) => (out += d.toString()));
child.stderr.on("data", (d) => (err += d.toString()));

const send = (msg) => child.stdin.write(JSON.stringify(msg) + "\n");

setTimeout(() => {
  send({ jsonrpc: "2.0", id: 1, method: "initialize", params: { protocolVersion: "2025-03-26", capabilities: {}, clientInfo: { name: "smoke", version: "0" } } });
}, 400);
setTimeout(() => {
  send({ jsonrpc: "2.0", id: 2, method: "tools/list", params: {} });
}, 900);
setTimeout(() => {
  const names = [...out.matchAll(/"name":"([a-z0-9_.]+)"/g)].map((m) => m[1]);
  const uniq = [...new Set(names)];
  console.log("RAW OUTPUT:\n" + out.slice(0, 2000));
  console.log("TOOL NAMES:", uniq.join(", "));
  console.log("TOOL COUNT:", uniq.length);
  const ok = out.includes('"tools"') && uniq.length >= 10;
  console.log(ok ? "MCP SMOKE PASS" : "MCP SMOKE FAIL");
  child.kill();
  process.exit(ok ? 0 : 1);
}, 1600);
