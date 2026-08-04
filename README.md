# Bondhive

> **Agents that promise. Bonds that hold.**

Bondhive is a **reliability layer for the agent economy**. It turns an
agent's word into a staked, attested, verifiable commitment — a *Service
Bond* — and settles it on-chain. Built on Nostr (NIP-74) and Solana, with
Buzz workspaces as the reference deployment.

**Status: design.** Nothing in this repository is live. Every component is
marked `design` (spec written) or `planned` (implementation pending). No
BOND tokens have been minted or launched.

## What

Agents (and humans) promise work all the time; nothing holds them to it.
Bondhive adds recourse:

- **Service Bonds (NIP-74, kinds 37000–37005).** An agent signs a bond
  promising specific work with specific terms — counterparty, amount,
  premium, expiry, required artifacts. The bond is a Nostr event, scoped to
  the channel (`h` tag) where the promise was made.
- **Witnesses.** Other members of the channel attest the bond is valid and
  the terms are coherent.
- **On-chain settlement.** A Solana program stakes, slashes, and rewards in
  **BOND** (1,000,000,000 supply, 9 decimals, pump.fun-launchable). Fulfill
  the bond and the stake returns plus premium; default and the stake is
  slashed and split.
- **BondScore.** Every resolution updates a reputation label (NIP-32, kind
  1985, namespace `bondhive`). Anyone can price a counterparty before
  accepting work.

## Why

The agent economy runs on trust with no enforcement. A hallucinating agent
can promise a deliverable, produce nothing, and walk away — there is no
identity revocation, no stake at risk, no reputation record. Bondhive makes
default expensive and reliability observable:

- **Evidence first.** Bonds live in the same event log as the work itself,
  so the audit trail (Buzz's hash-chained audit log) and the economic layer
  share one source of truth.
- **One identity.** The npub that signs the bond is the npub whose stake is
  at risk. No new accounts, no mapping tables.
- **Reputation that costs something.** BondScore is earned with fulfilled
  bonds, not self-asserted profiles.

## How

**Events first, settlement second.** Every lifecycle step is a signed Nostr
event: issuance (37000), attestation (37001), fulfillment (37002), dispute
(37003), resolution (37004), revocation (37005). An oracle agent watches
bonds and fulfillment claims, verifies artifacts deterministically, and
triggers the on-chain settlement. The Solana program is the escrow and
enforcer; the relay is the ledger.

```
agent promises work ──► kind 37000 (channel-scoped, h tag)
witnesses attest   ──► kind 37001
agent delivers     ──► kind 37002 (+ artifact refs)
oracle verifies    ──► on-chain resolve ──► BOND stake/reward/slash
resolutions feed   ──► BondScore labels (kind 1985, "bondhive")
```

## Architecture

Four layers, each independently replaceable:

| Layer | Piece | Status |
|-------|-------|--------|
| Event | NIP-74 kinds 37000–37005, NIP-32 labels, NIP-89 handler registration | design |
| Workspace | Buzz relay integration: kind registry, ingest scope gate, CLI, MCP | planned |
| Settlement | Solana program: stake / slash / reward / buy-back engine | planned |
| Reputation | BondScore derivation from resolution history | design |

## Repository layout

```
bondhive/
├── core/          # NIP-74 event schemas, BondScore math, event builders (Rust)   [planned]
├── program/       # Solana program: stake, slash, reward, buy-back (Rust)          [planned]
├── oracle/        # Oracle agent: watch → verify → resolve (Buzz member agent)     [planned]
├── buzz/          # Buzz integration: kinds, relay handlers, CLI, MCP server       [planned]
├── spec/          # NIP-74 draft, tokenomics, protocol design                      [design]
├── docs/          # Integration guides and runbooks
│   └── buzz-integration.md   # How Bondhive slots into Buzz (kinds, relay, CLI, MCP, oracle, rollout)
└── web/           # Dashboards and explorer                                          [planned]
```

## Quickstart

Prerequisites: Rust toolchain, Node.js ≥ 20, a Solana devnet RPC (for
settlement tests), and a Buzz checkout for the integration crates.

```bash
# 1. Core — NIP-74 schemas and BondScore math
cargo test -p bondhive-core

# 2. Program — host-logic tests run the full settlement math off-chain
cargo test -p bondhive-program            # includes host-logic suite

# 3. Buzz integration — kinds + ingest gate (run inside a buzz checkout)
cd /path/to/buzz
just ci                                    # fmt + clippy + tests before any PR
cargo test -p buzz-core -p buzz-relay      # exercises kind registration + scope gate

# 4. MCP server — Bondhive tools for Buzz agents (npm)
cd bondhive/buzz/mcp
npm install
npm run build

# 5. Web dashboard
cd bondhive/web
npm install
npm run build
```

### Verification commands

```bash
cargo test -p bondhive-core                      # schema + score unit tests
cargo test -p bondhive-program                   # settlement math (host-logic)
cargo test -p bondhive-program --lib             # program instruction unit tests
npm --prefix buzz/mcp test                       # MCP tool contract tests
npm --prefix web test                            # dashboard smoke tests
```

All tests must pass and `just ci` must be green (when touching the buzz
integration) before a PR.

## Tokenomics

| Parameter | Value | Status |
|-----------|-------|--------|
| Total supply | 1,000,000,000 BOND | design |
| Decimals | 9 | design |
| Launch | pump.fun (single-sided, open market) | planned |
| Bond premium | 1–5% of bond value (risk-tiered) | design |
| Slash split | 70% principal / 20% witnesses / 10% oracle | design |
| Buy-back engine | 50% of buy-back burned / 50% redistributed to stakers | design |
| Reward source | Premiums + buy-back redistribution | design |

Premium rates and the 70/20/10 slash split are protocol constants fixed in
`spec/tokenomics.md` (design); changing them post-launch requires the
program's upgrade authority and a governance signal.

## Documentation

- `docs/buzz-integration.md` — the full Buzz integration design: kind
  registration, ingest gate, `h`-tag scoping, MCP, oracle, CLI, relay
  config, migration plan.
- `spec/NIP-74.md` — Service Bond event spec (draft, design).
- `spec/tokenomics.md` — BOND economics (design).
- `AGENTS.md` — contributor guide for AI agents working on this repo.

## Status markers

| Marker | Meaning |
|--------|---------|
| `design` | Spec written; not implemented |
| `planned` | Implementation scheduled; not yet landed |
| `live` | Deployed and operating (none yet — everything above is design/planned) |

## License

TBD. See the project maintainers before contributing code that may need to
be relicensed.
