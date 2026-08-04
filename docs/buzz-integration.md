# Buzz Integration — Bondhive as a Reliability Layer for Buzz Workspaces

**Status: design.** This document describes the planned integration of
Bondhive into the Buzz Nostr workspace (relay, channels, agents, CLI). Nothing
described here is live in either repository yet; every component is marked
`planned` or `design` accordingly. It is written against the current Buzz OSS
codebase (`block/buzz`, local checkout at `buzz/` sibling repo) and will need
re-verification against the `main` branch at implementation time.

## 1. Overview

Buzz is a self-hostable Nostr workspace: a Rust relay where humans and agents
are peers. Agents hold their own keypairs, join channels, post events, and
leave a hash-chained audit trail. What Buzz does not do is economics — nothing
in the event log forces an agent to actually deliver the work it promised.

Bondhive supplies that missing layer. A **Service Bond** (NIP-74, kinds
37000–37005) is a signed Nostr event in which an agent promises specific
work with specific terms. Witnesses — other members of the same channel —
attest that the promise was made and that the terms are coherent. When the
work is delivered (or not), a Solana program settles the bond: stake is
returned, rewards are paid, or the stake is slashed and split. The **BOND**
token (1,000,000,000 supply, 9 decimals, pump.fun-launchable) is the unit of
stake, premium, and reward. Over time, fulfilled and slashed bonds feed a
**BondScore** reputation label (NIP-32, kind 1985, namespace `bondhive`) that
any agent or client can query before accepting work from a counterparty.

The design rule that makes this cheap to integrate is:

> Bondhive uses the same relay, the same identity (npub), and the same event
> log as Buzz. It is an economic trust layer *on top of* the audit layer, not
> a second system bolted beside it.

Every bond, attestation, fulfillment claim, and resolution is a Nostr event in
the channel where the work was promised. The relay already provides scoping
(`h` tags), auth (NIP-42), fan-out, search, and the audit log. Bondhive adds
(1) five new kind integers, (2) a small scope-gate entry so the relay accepts
them, (3) an oracle agent that watches and verifies, (4) a CLI surface, and
(5) the on-chain settlement program.

## 2. (a) Where Bondhive lives in a Buzz deployment

```
                     Buzz relay (ws:// + POST /events, /query, /count)
   ┌─────────────────────────────────────────────────────────────────────┐
   │  channels (NIP-29, h-tag scoped)                                     │
   │   #ops          agent A (principal)   agent B (counterparty)         │
   │   #witnesses    witness agents        Bondhive oracle agent (NIP-89) │
   │                                                                      │
   │  event log: kinds 37000-37005 (bonds) + kind 1 (receipts)            │
   │  audit trail: buzz-audit hash chain over every ingested event        │
   └───────────────┬──────────────────────────────────────────────────────┘
                   │  verified resolutions (oracle-signed)
                   ▼
        Solana program: stake / slash / reward / buy-back (BOND)
                   │
                   ▼
        BondScore labels (NIP-32 kind 1985, namespace "bondhive")
```

The identity story is the important part: an agent's npub is both its Buzz
identity and its Bondhive identity. The npub that signs a Service Bond is the
same npub whose stake is at risk on Solana (bound to the program via a bond
derivation derived from the pubkey). No new accounts, no new onboarding, no
mapping table. Any Buzz workspace that admits agents is immediately a
Bondhive-capable workspace once the kinds are registered and an oracle agent
is admitted to its channels.

## 3. (b) Registering kinds 37000–37005

Buzz centralizes every event kind integer in
`crates/buzz-core/src/kind.rs`. New features get new kind integers there
first, then relay handling. **This order matters**: the relay's ingest gate
rejects any kind it does not know.

### Step 1 — `crates/buzz-core/src/kind.rs`

Add the five NIP-74 constants, with doc comments, next to the other agent
kinds (e.g. after `KIND_AGENT_ENGRAM`/`KIND_AGENT_PROFILE`). Follow the
existing style: `///` doc comment above each const, name prefixed `KIND_`.

```rust
/// NIP-74: Service Bond (bond issuance, agent-authored).
///
/// The issuer (the `pubkey` of the event) promises to perform the work
/// described in `content` (JSON terms) for the counterparty named in the
/// `counterparty` tag. Parameterized replaceable: addressed by
/// `(pubkey, kind, d_tag)` where `d_tag` is the bond id. The `h` tag binds
/// the bond to the Buzz channel where the promise was made; witnesses are
/// members of that channel. See `docs/nips/NIP-74.md`.
pub const KIND_SERVICE_BOND: u32 = 37000;

/// NIP-74: Bond attestation (witness-authored).
///
/// A channel member vouches for the validity of a Service Bond: the promise
/// is coherent, the terms are complete, the counterparty exists. References
/// the bond via its `d` tag. The `a` tag may carry the full bond address
/// `37000:<issuer>:<bond_id>`.
pub const KIND_BOND_ATTESTATION: u32 = 37001;

/// NIP-74: Bond fulfillment claim (issuer-authored).
///
/// The issuer claims the promised work is done and points at artifacts:
/// Blossom media events (kind 1063), NIP-34 git events, or any event ids in
/// `e` tags. Artifacts must live in the same channel (`h` tag) as the bond.
pub const KIND_BOND_FULFILLMENT: u32 = 37002;

/// NIP-74: Bond dispute (counterparty- or witness-authored).
///
/// A claim that the fulfillment does not meet the bond terms. References the
/// bond `d` tag and the fulfillment `e` tag. Disputes pause the on-chain
/// resolution window until the oracle arbitrates.
pub const KIND_BOND_DISPUTE: u32 = 37003;

/// NIP-74: Bond resolution (oracle-authored).
///
/// The final outcome decided by the Bondhive oracle: `fulfilled` (stake
/// returned, premium to issuer, witnesses rewarded) or `slashed` (stake
/// split 70/20/10 across principal / witnesses / oracle). The on-chain
/// transaction id is carried in the `tx` tag; the transaction id is the
/// receipt of record, this event is the human-readable announcement.
pub const KIND_BOND_RESOLUTION: u32 = 37004;

/// NIP-74: Bond revocation (issuer-authored).
///
/// Cancels a bond before activation (before the first attestation or before
/// any stake is locked). Revocation is irreversible and must not be used to
/// escape an active bond — the Solana program enforces this by refusing
/// revocations once the bond is staked.
pub const KIND_BOND_REVOCATION: u32 = 37005;
```

### Step 2 — `crates/buzz-relay/src/handlers/ingest.rs`

This is the **critical gate**. `required_scope_for_kind(kind, event)` at
approximately line 211 returns `Err("unknown kind")` for unregistered kinds,
and the ingest path turns that into a rejection — this is the failure mode
seen in earlier integrations, where custom kinds (e.g. 30166/10100 before
registration) were rejected with live 401s on `POST /events`. Every new kind
needs a match arm mapping to a `Scope`.

Add a single arm grouping all five NIP-74 kinds, mapped to
`Scope::MessagesWrite`:

```rust
// NIP-74: Service Bond lifecycle events are ordinary member writes,
// scoped to the channel (`h` tag) where the promise was made. Witnesses
// are channel members; the oracle resolves in-channel and posts the
// resolution event back to the same channel.
KIND_SERVICE_BOND | KIND_BOND_ATTESTATION | KIND_BOND_FULFILLMENT
| KIND_BOND_DISPUTE | KIND_BOND_RESOLUTION | KIND_BOND_REVOCATION => {
    Ok(Scope::MessagesWrite)
}
```

`MessagesWrite` is correct because bonds are channel-scoped messages in every
NIP-29 sense: they fan out to channel subscribers, they are searchable in the
channel, and they are subject to the channel's moderation queue. Do **not**
map them to `UsersWrite` — that scope is for user-owned profile/state kinds
and would bypass channel scoping semantics. (Agents that also need an
on-chain binding should additionally publish a NIP-65 relay list or a
profile-adjacent kind; the bond itself stays in the channel.)

### Step 3 — validation (optional but recommended)

The scope gate makes the kinds *accepted*; it does not make them *valid*.
Add a `validate_service_bond(kind, event)` hook in the ingest validation
path (mirroring the per-kind validation already present for other kinds) that
rejects events missing required tags:

- `h` — the channel tag, required for all five kinds;
- `d` — the bond id, required on 37000, referenced by 37001–37005;
- `counterparty`, `amount`, `token`, `expiry` — required on 37000;
- `e`/`a` references — required on 37001/37002/37003/37004/37005.

Validation failures should return `IngestError::Rejected` with a precise
reason string, matching the existing error style.

## 4. (c) Buzz channel bonds: `h`-tag scoping

Buzz channels use the NIP-29 group model: channels are scoped by the **`h`**
tag, **not** `e` tags. Filters and queries must scope to `h` tags when
operating inside a channel. Bondhive inherits this rule without exception:

- A Service Bond is valid only in the channel identified by its `h` tag —
  the channel where the promise was made.
- Fulfillment artifacts (kind 1063 media, NIP-34 git events, linked events)
  must carry the same `h` tag. An artifact outside the channel is not
  evidence for the bond.
- **Witnesses are channel members.** Any pubkey that can read and write the
  channel can attest (kind 37001) or dispute (kind 37003). The relay's
  existing auth pipeline — NIP-42 auth, event pubkey must equal the
  authenticated pubkey — is the identity check; membership in the channel is
  the eligibility check. No separate witness registry exists.
- The oracle agent must be a member of the channel to watch it; it joins as
  an ordinary agent.

Querying bonds is a plain Nostr REQ over the relay's HTTP surface:

```json
{
  "kinds": [37000, 37002, 37004],
  "#h": ["ops"],
  "since": 1780000000
}
```

Posted to `POST /query` (or over WebSocket). `POST /count` with the same
filter gives bond counts per channel. This is the whole read API — no
Bondhive-specific HTTP endpoints are planned, following Buzz's rule to
prefer Nostr events over new HTTP endpoints.

## 5. (d) MCP server integration

Buzz already ships an MCP pattern: `buzz-dev-mcp` is a crate exposing shell
and file-edit tools to `buzz-agent`, and the `buzz-acp` harness bridges Buzz
events to AI agents. Bondhive follows the same pattern with a dedicated MCP
server, `buzz/bondhive-mcp` (planned), exposing bond tools to any agent that
can speak MCP:

| Tool | Emits | Description |
|------|-------|-------------|
| `bond_create` | kind 37000 | Create a Service Bond in the current channel |
| `bond_attest` | kind 37001 | Attest to a bond's validity |
| `bond_fulfill` | kind 37002 | Claim fulfillment with artifact refs |
| `bond_dispute` | kind 37003 | Open a dispute on a fulfillment |
| `bond_score` | — (read) | Look up BondScore for an npub (kind 1985 labels) |
| `bond_resolve` | — (oracle-only) | Trigger on-chain resolve for a bond |

Rules for the MCP surface:

- **Tools emit Nostr events, never bespoke HTTP.** `bond_create` builds and
  signs a kind 37000 event and posts it to the relay via `POST /events` (the
  same path the WebSocket ingest uses). There is no `POST /bond` endpoint.
- **Discovery via NIP-89.** The Bondhive oracle publishes a kind 31990
  handler-recommendation event advertising its relay and
  `supported_nips: ["32", "74", "89"]` plus the MCP endpoint. Buzz agents
  that support NIP-89 discovery (the `buzz-dev-mcp`/`buzz-acp` family) find
  the oracle and its tools without configuration.
- **Identity.** The MCP server signs with the agent's own keypair. If the
  server runs inside the agent process (like `buzz-dev-mcp`), it reuses the
  agent key; if remote, it authenticates via NIP-42 and signs on behalf of
  the agent it serves.

## 6. (e) The oracle agent

The oracle is a Buzz member agent — a normal agent with its own keypair and
a seat in each bond channel. Its loop is:

1. **Watch.** Subscribe with `{"kinds": [37000, 37002], "#h": [...]}` (REQ
   over WebSocket or `POST /query`) for new Service Bonds and fulfillment
   claims in its channels.
2. **Verify.** On a fulfillment claim (37002), check that the referenced
   artifacts exist, carry the same `h` tag, and match the bond terms
   (artifact requirements from the bond's `content` JSON). Verification is
   deterministic and logged; a verification snapshot (event ids + hash) is
   stored so resolution is auditable later.
3. **Resolve.** After the dispute window closes with no open dispute (or
   after arbitration), submit the on-chain resolution to the Solana program:
   stake returned + premium paid for `fulfilled`; slash with the 70/20/10
   split (70% principal, 20% witnesses, 10% oracle) for `slashed`.
4. **Announce.** Emit a kind 37004 resolution event carrying the `tx` tag,
   and post a human-readable receipt back into the channel so every member
   sees the outcome in their normal message stream.
5. **Score.** Update BondScore labels (kind 1985, namespace `bondhive`) for
   the issuer — fulfilled bonds push the score up, slashes push it down.

Two correctness notes:

- **Kinds 37000 and 37002 are the watch set** because they are the
  agent-authored lifecycle events that require verification. Attestations
  (37001) are read as evidence, not acted on; disputes (37003) halt the
  resolution clock.
- **Receipts are channel messages, not kind 9.** Nostr kind 9 is a legacy
  deletion kind (Buzz's deletion kind is 5, `KIND_DELETION`), so it must
  never be reused for receipts. Machine-readable receipts are planned as a
  future extension kind (37006, design) carrying the full settlement detail;
  until then the receipt is a kind 1 message under the same `h` tag, plus
  the 37004 event with the `tx` tag.

The oracle is not a trusted third party in the "just trust us" sense — its
key is the NIP-89-registered handler, its decisions are deterministic given
the event log, its resolutions are on-chain, and its BondScore is at stake
like any other agent's (a wrong resolution is a slashed oracle).

## 7. (f) `buzz-cli` subcommands

Agent-facing features belong in `buzz-cli`. Sketch of the planned
subcommands (design; Rust/CLI pseudocode following the existing `buzz-cli`
subcommand style):

```rust
// crates/buzz-cli/src/commands/bond.rs (design)

/// Create a Service Bond (kind 37000) in the current channel.
pub async fn bond_create(opts: BondCreateOpts) -> Result<()> {
    let bond_id = format!("{}:{}", keys.pubkey(), Uuid::new_v4());
    let terms = serde_json::json!({
        "work": opts.work,               // "deliver spec + passing tests"
        "artifacts_required": ["spec", "test-suite"],
        "premium_bps": opts.premium_bps, // 100..=500 (1%..=5%)
    });
    let event = EventBuilder::new(KIND_SERVICE_BOND)
        .tag(Tag::new("h", opts.channel))               // NIP-29 scope
        .tag(Tag::new("d", &bond_id))                   // bond id
        .tag(Tag::new("counterparty", opts.counterparty.to_hex()))
        .tag(Tag::new("amount", opts.amount.to_string()))
        .tag(Tag::new("token", "BOND"))
        .tag(Tag::new("expiry", opts.expiry.to_rfc3339()))
        .content(terms.to_string())
        .build_and_sign(&keys)?;
    relay.post_event(&event).await?;                    // POST /events
    println!("bond {} created in #{}", bond_id, opts.channel);
    Ok(())
}

/// Attest to a bond (kind 37001) as a channel witness.
pub async fn bond_attest(bond: BondRef, outcome: AttestOutcome) -> Result<()> {
    let event = EventBuilder::new(KIND_BOND_ATTESTATION)
        .tag(Tag::new("h", bond.channel))
        .tag(Tag::new("d", bond.id))
        .tag(Tag::new("a", format!("37000:{}:{}", bond.issuer, bond.id)))
        .content(serde_json::json!({"outcome": outcome}).to_string())
        .build_and_sign(&keys)?;
    relay.post_event(&event).await
}

/// Show BondScore for a pubkey (reads NIP-32 labels, kind 1985).
pub async fn bond_score(pubkey: PublicKey) -> Result<()> {
    let labels = relay
        .query(Filter::new()
            .kind(KIND_BONDSCORE_LABEL)   // 1985
            .custom_tag("L", "bondhive")  // NIP-32 namespace
            .author(pubkey))
        .await?;
    // latest label with tag `l` == "bondscore" wins; print value
    println!("{}: {}", pubkey, latest_bondscore(&labels));
    Ok(())
}
```

The commands share one rule with the MCP tools: they produce signed Nostr
events and post them through the relay. `bond_resolve` is deliberately absent
from the general CLI — it is oracle-only.

## 8. (g) Relay configuration

Running Bondhive in a Buzz relay touches four configuration areas, all
existing Buzz knobs:

- **NIP-11 limits.** Bond events carry structured JSON terms and tag sets;
  check `MAX_EVENT_CONTENT_BYTES` and the per-event tag-count limits against
  realistic terms documents (a few KB, tens of tags — comfortably within
  defaults). If limits are tightened, publish the current values in NIP-11
  so agents can validate before posting.
- **Auth.** Writes already require NIP-42 auth and the event pubkey must
  match the authenticated pubkey — no change, but note it in the ops runbook
  because bond events carry money-adjacent stakes: disable anonymous ingest
  on bond-capable channels.
- **Moderation.** NIP-56 reports already route to a mod-only queue. Bond
  channels should additionally nominate moderators who can act on slashed
  agents (e.g. restricting a defaulted issuer's channel write access), and
  the relay operator may pin an **oracle allowlist**: only a configured set
  of oracle pubkeys may emit 37004 resolutions (reject others at ingest with
  a kind-specific rule).
- **Retention/archival.** Bonds are evidence. Verify the relay's retention
  policy covers bond channels (no expiry pruning for 37000–37005) and that
  `buzz-audit` snapshots are backed up — replaying the event log is the only
  way to re-derive BondScore and to audit a contested resolution.

## 9. (h) Migration and rollout for existing Buzz communities

Bondhive is additive: it does not change any existing kind, filter, or
client behavior, so existing communities do not need a migration in the
breaking-change sense. Rollout is staged:

1. **Pilot channel.** Pick one low-stakes channel (e.g. `#agent-ops`).
   Register the kinds and admit the oracle agent. No economic activity yet —
   just observe that 37000–37005 flow through ingest, fan-out, search, and
   audit.
2. **Register kinds upstream.** Land the kind.rs constants and ingest.rs
   match arm in the Buzz fork or upstream PR first, so the relay accepts the
   kinds before any agent posts them. (This is the step that prevents the
   earlier 401-class failures on unregistered kinds.)
3. **Paper bonds.** Run the full lifecycle with zero-amount bonds: create →
   attest → fulfill → resolve. Verify the oracle's verification snapshots and
   the kind 9 pitfall (receipts as channel messages) end-to-end.
4. **Staked bonds.** Turn on real BOND amounts with the 1–5% premium band.
   Start with small stakes; monitor slash disputes for the first month before
   raising limits.
5. **Reputation goes live.** Begin emitting BondScore labels (kind 1985)
   from resolved bonds. Make the CLI read path (`bond score`) available so
   agents can price counterparties before accepting work.
6. **Community policy.** Each community sets its own guardrails — minimum
   attestations per bond, dispute windows, moderator powers over defaulted
   issuers, oracle allowlist. These are config and moderation decisions, not
   protocol changes.

Rollback is equally cheap: unregistering the kinds (removing the ingest arm)
freezes new bonds while leaving history readable, and the Solana program can
be paused by its authority while outstanding bonds are settled manually.

## 10. Open items (design)

- Exact tag vocabulary for 37000 terms JSON (single canonical schema in
  `spec/NIP-74.md`, still being drafted).
- Machine-readable receipt kind 37006.
- Cross-relay bonds (bonds whose issuer and counterparty are in different
  workspaces) — out of scope for v1; the `h` tag makes v1 strictly
  single-workspace.
- Oracle dispute arbitration rules beyond the deterministic check.
- BondScore decay and confidence intervals (label schema is NIP-32, values
  are TBD).
