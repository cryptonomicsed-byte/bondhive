/**
 * Bondhive parity self-test.
 *
 * Asserts the TypeScript economics mirror produces IDENTICAL numbers to the
 * Rust `logic` module (program/src/logic.rs). Run with `npm run build && node
 * dist/selftest.js`. Exit code 0 = parity confirmed.
 */

import * as econ from "./economics.js";
import * as events from "./events.js";

let failures = 0;
function check(name: string, actual: unknown, expected: unknown) {
  const ok = JSON.stringify(actual) === JSON.stringify(expected);
  if (!ok) {
    failures++;
    console.error(`FAIL ${name}: got ${JSON.stringify(actual)}, expected ${JSON.stringify(expected)}`);
  } else {
    console.log(`ok   ${name}`);
  }
}

// Parity with program/tests/logic.rs
check("premium_split 300", econ.premiumSplit(300), { claimsShare: 240, treasuryShare: 60 });
check("slash_split 10000", econ.slashSplit(10_000), { claimantShare: 7_000, witnessShare: 2_000, burnShare: 1_000 });
check("slash_split sums", (() => {
  const s = econ.slashSplit(333);
  return s.claimantShare + s.witnessShare + s.burnShare;
})(), 333);
check("slash_split 333", econ.slashSplit(333), { claimantShare: 233, witnessShare: 66, burnShare: 34 });
check("witness_fee 10000", econ.witnessFee(10_000), 50);
check("buyback_split 1000", econ.buybackSplit(1_000), { burn: 500, redistribute: 500 });
check("milestone_burn pre", econ.milestoneBurn(999_999, 1_000_000, 100_000), null);
check("milestone_burn post", econ.milestoneBurn(1_000_000, 1_000_000, 100_000), 500);
check("resolves_pass 3/2", econ.resolvesPass(3, 2, 3), true);
check("resolves_pass 3/1", econ.resolvesPass(3, 1, 3), false);
check("effective_premium 300@75", econ.effectivePremium(300, 75), 225);
check("effective_premium 300@50", econ.effectivePremium(300, 50), 300);
check("discount curve", [50, 60, 70, 80, 100].map(econ.premiumDiscount), [0, 10, 25, 50, 50]);

// Lifecycle happy path (mirror of Rust test)
const amount = 10_000;
const premium = econ.effectivePremium(300, 75);
const split = econ.premiumSplit(premium);
check("happy: premium after discount", premium, 225);
check("happy: claims share", split.claimsShare, 180);
check("happy: treasury share", split.treasuryShare, 45);

// Failure path
const slash = econ.slashSplit(amount);
check("fail: claimant", slash.claimantShare, 7_000);
check("fail: witnesses", slash.witnessShare, 2_000);
check("fail: burn", slash.burnShare, 1_000);

// Event builders
const ev = events.serviceBond({
  agentPubkey: "a".repeat(64),
  channel: "engineering",
  witnesses: ["w1".padEnd(64, "1")],
  taskType: "code-review",
  bondId: "b".repeat(32),
  amount: "10000",
  asset: "BOND",
  deadline: 1_800_000_000,
  createdAt: 1_700_000_000,
  description: "Review PR #42",
});
check("bond.kind", ev.kind, 37_000);
check("bond.d", ev.tags[0], ["d", "b".repeat(32)]);
check("bond.h", ev.tags[2], ["h", "engineering"]);
check("bond.witness", ev.tags.at(-1), ["witness", "w1".padEnd(64, "1")]);

const acc = events.acceptance({
  agentPubkey: "a".repeat(64),
  bondAddress: `37000:${"a".repeat(64)}:${"b".repeat(32)}`,
  amount: "10000",
  witnesses: [],
  createdAt: 1_700_000_100,
});
check("accept.a", acc.tags[0][0], "a");

if (failures > 0) {
  console.error(`\n${failures} parity failure(s)`);
  process.exit(1);
}
console.log("\nAll parity checks passed — TS economics == Rust logic.");
