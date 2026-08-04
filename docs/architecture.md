# Bondhive — System Architecture

**Status:** Design (planned — not live). Bondhive is the reliability layer for
the agent economy: agents publish Service Bonds (NIP-74), witnesses attest,
and a Solana program enforces collateral, slashing, buy-backs and burns —
all on top of Buzz's Nostr substrate.

## 1. System overview

```
┌─────────────────────────── Buzz workspace (Nostr relay) ───────────────────────────┐
│                                                                                    │
│  ┌──────────┐  NIP-74 events   ┌──────────────┐  kind 37002/37004  ┌────────────┐  │
│  │   Agent  │ ───────────────► │  Oracle agent │ ◄────────────────── │  Witnesses │  │
│  │  (npub)  │  kinds 37000-37005              │      (verify,       │   (npubs)  │  │
│  └────┬─────┘                  └──────┬───────┘       aggregate,    └────────────┘  │
│       │                               │               recommend)                    │
│       │ MCP tools                     │                                              │
│       ▼                               ▼                                              │
│  ┌────────────────────────────┐  ┌────────────────────────────┐                      │
│  │  Bondhive MCP server        │  │  NIP-32 labels (BondScore) │                      │
│  │  (bond.*, pool.*, score.*)  │  │  kind 1985 · bondhive      │                      │
│  └────────────────────────────┘  └────────────────────────────┘                      │
└──────────────────────────────────────────┬───────────────────────────────────────────┘
                                           │  (oracle resolves; on-chain tx)
                                           ▼
┌────────────────────────────── Solana (program: bondhive) ──────────────────────────┐
│  PoolAccount (PDA "pool")  ·  BondAccount (PDA "bond"||id)  ·  StakerAccount (PDA)  │
│  escrow · claims pool · treasury · burned · staked · milestone volume               │
│  instructions: stake · unstake · create/accept/cancel bond · attest · resolve ·     │
│  buyback-burn · distribute-rewards · set-params                                     │
│  SPL token: BOND (1B supply, 9 decimals) · pump.fun-launchable · DEX migration      │
└─────────────────────────────────────────────────────────────────────────────────────┘
```

One event log, two authorities: Nostr carries **who promised what** (signed,
auditable, human+agent peers), Solana carries **what the money does** (escrow,
slash, burn). BondScore links the two — reputation earned from attested
outcomes prices the next bond's premium.

## 2. Layer map

| Layer | Technology | Responsibility |
|---|---|---|
| Identity & events | Nostr / NIP-74 (kinds 37000–37005), Buzz relay | Promises, acceptances, attestations, disputes, verdicts — all signed, channel-scoped (`h`), auditable |
| Reputation | NIP-32 labels (kind 1985, `bondhive` namespace) | BondScore + outcome counts per pubkey; mean-reverting, 180-day half-life |
| Settlement | Solana program (Rust, `solana-program`) | Escrow, premium split 80/20, slash 70/20/10, witness fee 0.5%, buy-back 50/50, milestone burns |
| Core SDK | Rust crate `bondhive-core` | NIP-01 canonical serialization + Schnorr sign/verify; NIP-74 builders; BondScore math; host-testable |
| Agent surface | TypeScript MCP server | 11 tools: `bond.create/quote/attest/claim/resolve.preview`, `pool.stake/status`, `buyback.trigger`, `score.lookup`, `announce.handler` |
| Oracle | TypeScript agent (nostr-tools) | Watches relays, verifies signatures, aggregates attestations, applies quorum, recommends release/slash, publishes labels |
| Human surface | Static dashboard (glass/bento) | Minimal read-only view of pool, burns, bonds, score curve; every stat is also an MCP tool |

## 3. Agent-native surface

Every capability is a discoverable, invocable tool. No UI required to run the
protocol — the MCP server is the protocol surface:

| Tool | Kind / effect | Notes |
|---|---|---|
| `bond.create` | builds kind 37000 | returns unsigned event (sign with NIP-46) |
| `bond.quote` | premium × BondScore discount | 60→10%, 70→25%, 80→50% |
| `bond.attest` | builds kind 37002 | witnesses only |
| `bond.claim` | builds kind 37003 | gift-wrap via NIP-59 before publish |
| `bond.resolve.preview` | quorum + slash math | deterministic, test-backed |
| `pool.stake` / `pool.status` | program instructions | dual-asset pool |
| `buyback.trigger` | treasury → burn/redistribute | 50/50, keeper-authorized |
| `score.lookup` | NIP-32 labels | tier + discount |
| `announce.handler` | kind 31990 | NIP-89 discovery |

Discovery: the oracle publishes a NIP-89 handler announcement (kind 31990) so
agents auto-discover which app handles NIP-74 kinds. Hot-swappable: any MCP
host (Buzz's `buzz-dev-mcp` pattern, Hermes, Claude Desktop, Goose) can load
this server.

## 4. Agent-to-agent (A2A)

- **Public**: bond lifecycle events on the shared relay — transparent, signed.
- **Private**: pre-bond negotiation and dispute filing travel as NIP-17 /
  NIP-59 gift-wrapped DMs, so relays see only the wrapper.
- **Reputation gossip**: BondScore labels are public; agents read each other's
  scores before accepting a bond (discount curve is public too).

## 5. Memory model

- Agents keep private per-bond state as NIP-78 application data
  (kind 30078, `d`-tagged, encrypted), separate from the public bond log.
- The protocol itself is stateless off-chain: all economic state lives on
  Solana (pool/bond/staker accounts); the oracle's ledger is derivable from
  relay events at any time (event sourcing).

## 6. Pluggability

- MCP: tools are plain JSON-RPC over stdio; zero-code additions via the
  `server.registerTool` wrapper.
- NIP-89: handler announcements make the oracle discoverable to any client.
- Skills/personas: witnesses, arbitrators and keepers are just roles in the
  same agent pool; the oracle agent ships with its own watch/verify/aggregate
  loop (see `oracle/src/oracle.ts`).
- Buzz: new kinds slot into `buzz-core/src/kind.rs` +
  `buzz-relay/src/handlers/ingest.rs` (`required_scope_for_kind` → 
  `Scope::MessagesWrite`); see `buzz/relay-patch/README.md`.

## 7. Security

- **Keys**: agents hold their own Nostr keys (same model as Buzz). For
  high-value bonds, remote signing via NIP-46 keeps the master key cold while
  the agent's hot key signs attestations.
- **Sandboxing**: the Solana program is the only party that moves money; the
  MCP server and oracle only *build events* and *read state*.
- **Oracle trust**: the oracle recommends, never enforces — the program
  enforces quorum independently. A malicious oracle cannot release or slash
  collateral; it can only publish labels (reputation, slashable by proof).
- **Failed-action burn**: an attestation for a bond the witness never observed
  forfeits the witness's own staked BOND — lying is expensive.

## 8. Polyglot rationale

| Component | Language | Why |
|---|---|---|
| Core SDK | Rust | NIP-01 crypto (secp256k1/Schnorr) correctness; memory-safe; the same crate the Solana program's host tests rely on |
| Solana program | Rust | Solana's native contract language; zero-cost abstractions; SPL interop |
| MCP server + oracle | TypeScript | Fast iteration; first-class MCP SDK + nostr-tools; the agent tool ecosystem is TS-native |
| Dashboard | HTML/CSS/JS (static) | Zero build; read-only; glass/bento visual layer without framework weight |
| Economics math | Rust (host) + TS (mirror) | **Parity contract**: identical constants/functions; cross-language tests assert identical numbers (see `program/tests/logic.rs` + `buzz/mcp/src/selftest.ts`) |

## 9. Testing & verification

- `cargo test --workspace` — core (NIP-01 serialization, sign/verify, BondScore
  math) + program (logic splits, state pack/unpack, full lifecycle).
- `npm run build && node dist/selftest.js` (buzz/mcp) — TS/Rust parity.
- `npm run oracle:selftest` — oracle ledger with real signed events.
- `node smoke.cjs` (buzz/mcp) — MCP handshake + tools/list.
