# AGENTS.md — AI Agent Contributor Guide

This guide is for AI agents contributing to the Bondhive codebase. It
complements (and where it conflicts, overrides) generic contributor docs.
Everything here is written for the repo **as designed** — several components
are still `planned`, so when a section says "the relay's ingest gate", it
means the code you are about to write or modify, not code that exists yet.

## Where things live

| Path | Contents |
|------|----------|
| `core/` | NIP-74 event schemas (kinds 37000–37005), BondScore math, event builders. Pure Rust, no I/O. |
| `program/` | Solana program: stake, slash, reward, buy-back engine. Settlement math must live in host-testable (non-`solana_program`) modules. |
| `oracle/` | The oracle agent: watches kind 37000/37002 events, verifies artifacts, triggers on-chain resolve, posts receipts to the channel. |
| `buzz/` | Buzz integration: kind registry additions, relay ingest handler, `buzz-cli` subcommands, `buzz/mcp` MCP server. |
| `spec/` | NIP-74 draft, tokenomics, protocol design docs. Any semantic change to bonds must update `spec/NIP-74.md` in the same PR. |
| `docs/` | Integration guides (see `docs/buzz-integration.md` for the Buzz wiring). |
| `web/` | Dashboards. Read-only consumers of events and on-chain state. |

## The event-first rule

**New features are new event kinds, not new endpoints.** The Bondhive
protocol is a set of Nostr events. If a feature changes what an agent can
promise, attest, claim, dispute, or resolve, it changes a kind or adds one —
never a bespoke HTTP endpoint. This matches the Buzz convention (see the
Buzz repo's own AGENTS.md): Nostr events over HTTP endpoints, because events
get realtime fan-out, NIP-29 scoping, and the existing auth pipeline for
free.

Two hard gates to remember when adding a kind:

1. **`crates/buzz-core/src/kind.rs`** (in a Buzz checkout, upstream or a
   fork): every kind integer is registered here first, with a doc comment
   explaining the kind and its tags.
2. **`crates/buzz-relay/src/handlers/ingest.rs`,
   `required_scope_for_kind`** (≈ line 211): unregistered kinds are rejected
   at ingest (`Err("unknown kind")` → rejection surfaced as a 401/400-class
   error on `POST /events`). Every new kind needs a match arm mapping to a
   `Scope`. NIP-74 kinds 37000–37005 map to `Scope::MessagesWrite` — they
   are channel-scoped member writes. This gate is why "just post a custom
   kind" integrations fail; the relay must know the kind before it accepts
   it.

When you add a kind to Bondhive, you are responsible for all four touch
points: `kind.rs` constant, `ingest.rs` scope arm, validation hook (reject
events missing required tags with a precise `Rejected` reason), and the
NIP-74 spec section in `spec/NIP-74.md`.

## Extending NIP-74

- Kinds 37000–37005 are the core lifecycle; they are **channel-scoped by the
  `h` tag**. Bond events must carry `h` and `d` tags; fulfillment artifacts
  must share the bond's `h` tag. Filters must scope to `#h` inside a
  channel — never `e`-tag scoping.
- Extension kinds stay in the 37xxx block (e.g. 37006, the planned
  machine-readable receipt kind). Add them to the spec, the registry, and
  the ingest gate in one PR — never piecemeal.
- Semantic changes to existing kinds are breaking changes for every witness
  and oracle in the field; prefer a new kind over reinterpreting an old one.

## MCP tool registration

Agent-facing tools live in `buzz/mcp` (npm, following the `buzz-dev-mcp`
pattern: tools emit events, the server signs with the agent's own keypair).
For every new tool:

1. Register it in the MCP server's tool list with a name, description, and
   JSON input schema — the schema **is** the contract; agents discover it,
   so a vague schema is a bug.
2. Tools **emit Nostr events via `POST /events`** — never new HTTP
   endpoints. `bond_create` builds a signed kind 37000 event; it does not
   call a Bondhive REST API.
3. Update the tool table in `docs/buzz-integration.md` and the README.
4. Oracle-only actions (`bond_resolve`) must be gated by oracle key
   verification — never exposed to arbitrary agents.

## Test conventions

- **`core/`**: unit tests with `cargo test -p bondhive-core`. Event fixtures
  are JSON golden files in `core/tests/fixtures/`; schema changes update
  fixtures in the same PR.
- **`program/`**: settlement math (premium bands, slash splits, buy-back
  distribution) lives in plain Rust modules compiled for the host, tested
  with `cargo test -p bondhive-program` — **host-logic tests are mandatory**
  for any money math. Instruction handlers stay thin and delegate to these
  modules. Write host-logic tests first (red), then the program code (green).
- **`buzz/`**: run `just ci` (fmt + clippy + tests) in the Buzz checkout
  before any PR touching the integration. Integration tests that need a live
  relay run under `just test`.
- **`oracle/`**: verification logic (artifact checks, dispute windows) is
  pure and unit-tested; the event-loop shell is thin.

## Quality gates (Bondhive + Buzz conventions)

- **Commit with `git commit -s`.** The required DCO check fails any commit
  missing a `Signed-off-by` trailer. If you build commit commands
  programmatically, include `-s` every time; repair unsigned branches with
  `git rebase --signoff main`.
- **No `unsafe` code** anywhere.
- **No new `unwrap()` or `expect()` in production paths** — use `?` and
  proper error types. Tests may use them; production may not.
- **New public API must have doc comments** (`///` on every public item).
- **`just ci` green before PRs.** Clippy passing does not mean fmt passes;
  run both.
- **Statuses must be honest.** This repo marks everything not yet deployed as
  `design` or `planned`. Do not claim something is `live`, tested, or
  launched when it is not. If you add a component, mark it `planned` until
  it ships and is verified.

## Working with the Buzz repo

The integration touches two repositories. When changing the Buzz side:

- The Buzz checkout is a sibling repo; treat its own AGENTS.md as
  authoritative for that repo.
- Activate the Hermit toolchain (`. ./bin/activate-hermit`) before running
  `just` or hooks; do not rewrite hook commands to work around a missing
  PATH.
- Never fork-and-drift: kind additions and ingest arms should be proposed
  upstream (or tracked as an explicit patch file in `buzz/`) so the relay
  accepts the kinds before any agent posts them.

## Definition of done

A contribution is done when: the kind or change is registered in
`kind.rs` **and** the ingest gate **and** the NIP-74 spec **and** (if
agent-facing) the MCP tool list and CLI; fixtures and host-logic tests
cover it; docs (README status table, `docs/buzz-integration.md`) are
updated; all tests pass; and the commit is signed with `-s`.
