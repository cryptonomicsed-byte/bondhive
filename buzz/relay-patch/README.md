# Wiring NIP-74 into the Buzz relay

Bondhive lives *on top of* Buzz: same relay, same identity (npub), same event
log. For the new kinds to pass ingestion, the relay must know them. This file
is the exact, verified patch — grounded in the local checkout at
`/Users/bino/buzz` (block/buzz OSS).

## The gate

`crates/buzz-relay/src/handlers/ingest.rs` — `required_scope_for_kind()`
(≈ line 211) is a `match kind { ... }` that **returns `Err("unknown kind")`
for anything not registered**. That is the live-401 seam: a stock Buzz relay
rejects custom kinds at the door. Two edits are required.

## 1. Register the kinds — `crates/buzz-core/src/kind.rs`

Add these constants alongside the other `KIND_*` constants (the file is the
authoritative kind registry; constants are `u32`):

```rust
// ---------------------------------------------------------------------------
// NIP-74 — Agent Bonds and Attestations (Bondhive)
// ---------------------------------------------------------------------------

/// NIP-74: Service Bond — an agent's promise, backed by collateral.
/// Addressable (NIP-33) keyed by `d` = bond id. Channel-scoped via `h` tag.
pub const KIND_SERVICE_BOND: u32 = 37000;
/// NIP-74: Bond Acceptance — agent commits; collateral staked on-chain.
pub const KIND_BOND_ACCEPTANCE: u32 = 37001;
/// NIP-74: Performance Attestation — witness records pass/fail for a bond.
pub const KIND_PERFORMANCE_ATTESTATION: u32 = 37002;
/// NIP-74: Bond Claim (dispute) — gift-wrapped inner event (NIP-59).
pub const KIND_BOND_CLAIM: u32 = 37003;
/// NIP-74: Arbitration Verdict — resolves a bond dispute.
pub const KIND_ARBITRATION_VERDICT: u32 = 37004;
/// NIP-74: Bond Cancellation — valid only before acceptance.
pub const KIND_BOND_CANCELLATION: u32 = 37005;
```

`37000–37005` sits in the NIP-33 parameterized-replaceable range
(`is_parameterized_replaceable` already handles 30000–39999), so relays get
latest-per-`(pubkey, kind, d)` semantics for bonds for free.

## 2. Map them to a scope — `crates/buzz-relay/src/handlers/ingest.rs`

Add an arm to `required_scope_for_kind()`:

```rust
// NIP-74 agent bonds — ordinary member writes, channel-scoped by `h`.
k if (37000..=37005).contains(&k) => Ok(Scope::MessagesWrite),
```

`MessagesWrite` lets any channel member publish bond lifecycle events under
their own key — same trust model as a message. If a community wants
moderator-gated bonds, swap the arm for `Scope::AdminChannels` per-community.

## 3. Optional: index `a`-tag filters

Bonds and their attestations cross-reference each other via `a` tags
(`37000:<pubkey>:<bond-id>`). Ensure the relay indexes single-letter tags
(`a`, `h`, `p`, `v`, `witness`) so `REQ {"kinds":[37002],"#a":[addr]}` returns
the full attestation set for a bond — the oracle agent depends on it.

## 4. Verification

```bash
cd /Users/bino/buzz
just ci                      # fmt + clippy + unit tests
just test                    # integration tests (needs Postgres + Redis)
# Manual smoke:
#   publish a kind 37000 event via POST /events → expect 200
#   REQ {"kinds":[37000],"#h":["engineering"]} → expect the bond back
```

## Rollout

1. Ship the patch in the relay release; Bondhive events are inert to other
   clients (new kinds, ignored until a NIP-74 client reads them).
2. Publish the Bondhive handler announcement (kind 31990) so agents discover
   the MCP server (see `spec/NIP-74.md`).
3. Turn on the oracle agent in a channel; bonds become live there.

No changes to auth, search, or moderation are required — new kinds ride the
existing event pipeline.
