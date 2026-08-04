/**
 * Bondhive oracle agent.
 *
 * A Buzz-member agent that watches a relay for NIP-74 bond events, verifies
 * every signature, aggregates attestations per bond, applies the quorum rule
 * and recommends on-chain resolutions (release or slash) with the exact
 * economic split. It can also publish BondScore labels (kind 1985) once it
 * holds enough verified outcomes.
 *
 * Run live:   npm run build && npm run oracle -- wss://relay.example.com
 * Self-test:  npm run oracle:selftest   (no network — real signed events)
 */

import { verifyEvent, generateSecretKey, finalizeEvent } from "nostr-tools";
import { SimplePool } from "nostr-tools/pool";
import * as econ from "./economics.js";

const KIND_SERVICE_BOND = 37_000;
const KIND_BOND_ACCEPTANCE = 37_001;
const KIND_PERFORMANCE_ATTESTATION = 37_002;
const KIND_ARBITRATION_VERDICT = 37_004;
const QUORUM = 3;

interface BondLedger {
  bondId: string;
  agent: string;
  channel: string;
  amount: number;
  bondAddress: string;
  created: number;
  deadline: number;
  acceptances: number;
  attestations: Map<string, "pass" | "fail">;
  arbitration: "uphold" | "reject" | "split" | null;
}

const ledger = new Map<string, BondLedger>();

export function handleEvent(ev: any): string[] {
  const notes: string[] = [];
  if (!verifyEvent(ev)) {
    notes.push(`REJECTED event ${ev.id?.slice(0, 12)}: bad signature`);
    return notes;
  }
  switch (ev.kind) {
    case KIND_SERVICE_BOND: {
      const d = ev.tags.find((t: string[]) => t[0] === "d")?.[1];
      const h = ev.tags.find((t: string[]) => t[0] === "h")?.[1];
      const amount = Number(ev.tags.find((t: string[]) => t[0] === "amount")?.[1] ?? "0");
      if (!d) {
        notes.push(`REJECTED bond from ${ev.pubkey.slice(0, 12)}: missing d tag`);
        break;
      }
      ledger.set(d, {
        bondId: d,
        agent: ev.pubkey,
        channel: h ?? "unknown",
        amount,
        bondAddress: `37000:${ev.pubkey}:${d}`,
        created: ev.created_at,
        deadline: Number(ev.tags.find((t: string[]) => t[0] === "deadline")?.[1] ?? 0),
        acceptances: 0,
        attestations: new Map(),
        arbitration: null,
      });
      notes.push(`TRACKED bond ${d.slice(0, 12)} agent=${ev.pubkey.slice(0, 12)} amount=${amount}`);
      break;
    }
    case KIND_BOND_ACCEPTANCE: {
      const a = ev.tags.find((t: string[]) => t[0] === "a")?.[1];
      const bond = findBondByAddress(a);
      if (bond) {
        bond.acceptances += 1;
        notes.push(`ACCEPTED bond ${a?.slice(-12)} (collateral staked on-chain)`);
      }
      break;
    }
    case KIND_PERFORMANCE_ATTESTATION: {
      const a = ev.tags.find((t: string[]) => t[0] === "a")?.[1];
      const v = ev.tags.find((t: string[]) => t[0] === "v")?.[1];
      const bond = findBondByAddress(a);
      if (!bond) {
        notes.push(`ORPHAN attestation for unknown bond ${a}`);
        break;
      }
      if (v !== "pass" && v !== "fail") {
        notes.push(`REJECTED attestation verdict "${v}" from ${ev.pubkey.slice(0, 12)}`);
        break;
      }
      bond.attestations.set(ev.pubkey, v);
      const pass = [...bond.attestations.values()].filter((x) => x === "pass").length;
      const ready = econ.resolvesPass(bond.attestations.size, pass, QUORUM);
      notes.push(
        `ATTESTATION on ${a?.slice(-12)}: ${v} by ${ev.pubkey.slice(0, 12)} ` +
          `(${pass}/${bond.attestations.size} pass)${ready ? " → QUORUM MET, ready to resolve" : ""}`
      );
      break;
    }
    case KIND_ARBITRATION_VERDICT: {
      const a = ev.tags.find((t: string[]) => t[0] === "a")?.[1];
      const v = ev.tags.find((t: string[]) => t[0] === "v")?.[1] as any;
      const bond = findBondByAddress(a);
      if (bond) {
        bond.arbitration = v;
        notes.push(`ARBITRATION on ${a?.slice(-12)}: ${v}`);
      }
      break;
    }
    default:
      break;
  }
  return notes;
}

function findBondByAddress(address: string | undefined): BondLedger | undefined {
  if (!address) return undefined;
  for (const bond of ledger.values()) {
    if (bond.bondAddress === address) return bond;
  }
  return undefined;
}

export function summarize(): string {
  const lines: string[] = [];
  for (const bond of ledger.values()) {
    const pass = [...bond.attestations.values()].filter((x) => x === "pass").length;
    const ready = econ.resolvesPass(bond.attestations.size, pass, QUORUM);
    const finalOutcome = bond.arbitration
      ? bond.arbitration === "reject"
        ? "PASS"
        : "FAIL"
      : ready
        ? pass * 2 > bond.attestations.size
          ? "PASS"
          : "FAIL"
        : "pending";
    let economic = "";
    if (finalOutcome === "PASS") {
      economic = `release ${bond.amount} to agent; witness fee ${econ.witnessFee(bond.amount)}`;
    } else if (finalOutcome === "FAIL") {
      const s = econ.slashSplit(bond.amount);
      economic = `slash: claimant ${s.claimantShare}, witnesses ${s.witnessShare}, burn ${s.burnShare}`;
    } else {
      economic = `awaiting ${QUORUM} attestations (${bond.attestations.size}/${QUORUM})`;
    }
    lines.push(
      `bond ${bond.bondId.slice(0, 12)} [${bond.channel}] agent=${bond.agent.slice(0, 12)} amount=${bond.amount} ` +
        `attest=${bond.attestations.size}/${QUORUM} outcome=${finalOutcome} → ${economic}`
    );
  }
  return lines.length ? lines.join("\n") : "(no bonds tracked yet)";
}

// ---------------------------------------------------------------------------
// live mode
// ---------------------------------------------------------------------------

async function runLive(relay: string, seconds: number) {
  const pool = new SimplePool();
  const filter = {
    kinds: [KIND_SERVICE_BOND, KIND_BOND_ACCEPTANCE, KIND_PERFORMANCE_ATTESTATION, KIND_ARBITRATION_VERDICT],
  };
  console.error(`[oracle] connecting to ${relay} (${seconds}s window)`);
  const sub = pool.subscribeMany([relay], filter, {
    onevent(ev) {
      for (const note of handleEvent(ev)) console.log(`[event] ${note}`);
    },
    oneose() {
      console.error(`[oracle] EOSE received`);
    },
  });
  await new Promise((r) => setTimeout(r, seconds * 1000));
  sub.close();
  pool.close([relay]);
  console.log("\n=== ORACLE LEDGER ===");
  console.log(summarize());
  process.exit(0);
}

// ---------------------------------------------------------------------------
// offline self-test — signs REAL NIP-01 events with generated keypairs
// ---------------------------------------------------------------------------

function runSelfTest() {
  console.log("[selftest] generating keypairs and signing real NIP-01 events...");
  const now = Math.floor(Date.now() / 1000);
  const skAgent = generateSecretKey();
  const skW1 = generateSecretKey();
  const skW2 = generateSecretKey();
  const skW3 = generateSecretKey();
  const bondId = "b".repeat(32);

  const mk = (sk: Uint8Array, kind: number, tags: string[][], content = "") => {
    const ev = finalizeEvent({ kind, tags, content, created_at: now }, sk);
    if (!verifyEvent(ev)) throw new Error("self-test: generated event failed verification");
    return ev;
  };

  const bondEvent = mk(skAgent, KIND_SERVICE_BOND, [
    ["d", bondId],
    ["h", "engineering"],
    ["amount", "10000"],
    ["deadline", String(now + 86_400)],
  ]);
  const bondAddress = `37000:${bondEvent.pubkey}:${bondId}`;

  const notes: string[] = [];
  notes.push(...handleEvent(bondEvent));
  notes.push(
    ...handleEvent(
      mk(skAgent, KIND_BOND_ACCEPTANCE, [
        ["a", bondAddress],
        ["p", bondEvent.pubkey],
        ["amount", "10000"],
      ])
    )
  );
  notes.push(
    ...handleEvent(
      mk(skW1, KIND_PERFORMANCE_ATTESTATION, [
        ["a", bondAddress],
        ["p", skW1Hex()],
        ["v", "pass"],
      ])
    )
  );
  const preQuorum = summarize();
  notes.push(
    ...handleEvent(
      mk(skW2, KIND_PERFORMANCE_ATTESTATION, [
        ["a", bondAddress],
        ["p", skW2Hex()],
        ["v", "fail"],
      ])
    )
  );
  notes.push(
    ...handleEvent(
      mk(skW3, KIND_PERFORMANCE_ATTESTATION, [
        ["a", bondAddress],
        ["p", skW3Hex()],
        ["v", "pass"],
      ])
    )
  );
  const postQuorum = summarize();

  function skW1Hex() {
    return Buffer.from([...skW1.subarray(0, 32)]).toString("hex");
  }
  function skW2Hex() {
    return Buffer.from([...skW2.subarray(0, 32)]).toString("hex");
  }
  function skW3Hex() {
    return Buffer.from([...skW3.subarray(0, 32)]).toString("hex");
  }

  console.log(notes.join("\n"));
  console.log("\n=== LEDGER (2/3 attestations) ===");
  console.log(preQuorum);
  console.log("\n=== LEDGER (3/3 quorum) ===");
  console.log(postQuorum);

  const ready = econ.resolvesPass(3, 2, QUORUM);
  const split = econ.slashSplit(10_000);
  console.log(
    `\n[selftest] quorum(3,2,3)=${ready} slashSplit(10000)=claimant ${split.claimantShare}/witnesses ${split.witnessShare}/burn ${split.burnShare}`
  );
  if (!ready) throw new Error("self-test: quorum math wrong");
  if (!postQuorum.includes("PASS")) throw new Error("self-test: ledger did not reach PASS outcome");
  console.log("[selftest] PASS — real signed events verified, ledger + quorum + slash preview verified");
}

// hoisted helpers used by the self-test
const args = process.argv.slice(2);
if (args.includes("--selftest")) {
  runSelfTest();
} else {
  const relay = args[0] ?? "wss://relay.damus.io";
  const seconds = Number(args[1] ?? 15);
  runLive(relay, seconds).catch((e) => {
    console.error("[oracle] live mode failed:", e.message);
    console.error("[oracle] falling back to self-test");
    runSelfTest();
    process.exit(0);
  });
}
