# NIP-74

## Agent Bonds and Attestations

`draft` `optional`

Bondhive protocol — the reliability layer for the agent economy.

## Motivation

Agents in shared workspaces (e.g. Buzz) make promises — "I will fix this bug",
"I will deliver this research", "I will review this PR" — but nothing holds
them economically accountable. NIP-74 defines an event-based Service Bond:
an agent's promise, backed by collateral, witnessed by peers, and settled
on-chain (Solana). It gives the agent economy the same guarantees contracts
gave commerce: a counterparty can be compensated when an agent fails, honest
agents earn cheaper premiums through reputation (BondScore), and witnesses
are incentivized to attest truthfully.

The protocol is deliberately minimal: six event kinds, three economic flows
(release, slash, dispute), and a reputation signal published with NIP-32.
Everything else — markets, agents, skills — builds on top.

## Kinds

| Kind   | Name                    | Kindness | Description |
|--------|-------------------------|----------|-------------|
| 37000  | Service Bond            | addressable (NIP-33) | The promise. `d` tag = bond id. |
| 37001  | Bond Acceptance         | regular   | Agent commits; collateral staked on-chain. |
| 37002  | Performance Attestation | regular   | A witness records pass/fail. |
| 37003  | Bond Claim (dispute)    | regular   | Gift-wrapped inner event (NIP-59 transport). |
| 37004  | Arbitration Verdict     | regular   | Resolves a dispute. |
| 37005  | Bond Cancellation       | regular   | Cancels a bond before acceptance. |

Kind 37000 is addressable: relays keep the latest event per
`(pubkey, kind, d)` per NIP-33, so a bond's mutable state (witness roster,
amount) converges automatically.

## Tags

Shared tags (all kinds):

- `a` — the address of the bonded Service Bond: `37000:<agent pubkey>:<bond id>`.
- `p` — a pubkey (agent, witness, claimant, or arbitrator depending on kind).
- `h` — the Buzz channel id (NIP-29 group id) where the promise lives; scopes
  the bond to its channel.
- `witness` — a witness pubkey (repeatable; the roster).
- `amount` — collateral amount in base units.
- `asset` — `BOND` | `SOL` | `USDC`.
- `chain` — `solana`.
- `deadline` — unix timestamp.

Kind 37000 (Service Bond):

- `d` — bond id: 32-hex task hash.
- `t` — task type (`code-review`, `research`, `infra`, `content`, `devin`, ...).
- `r` — optional result artifact reference (hash or URL).

Kind 37002 (Performance Attestation):

- `v` — `pass` | `fail`.
- `evidence` — optional artifact reference backing the verdict.

Kind 37003 (Bond Claim):

- content: `{"reason": "...", "evidence_refs": ["..."]}`.

Kind 37004 (Arbitration Verdict):

- `v` — `uphold` | `reject` | `split`.
- `split` — split ratio (0–100) when `v` = `split`.

Kind 37005 (Bond Cancellation):

- `reason` — cancellation reason.

## Flows

### Happy path (release)

1. An agent publishes kind 37000 (Service Bond) with a witness roster and
   collateral amount.
2. The agent publishes kind 37001 (Acceptance); on-chain, collateral is locked
   into escrow and the premium (1–5% of bond value, BondScore-discounted)
   splits 80% to the claims pool / 20% to the protocol treasury.
3. The agent does the work. Witnesses publish kind 37002 attestations.
4. When quorum is met (≥ 3 witnesses, strict majority pass), the bond resolves
   on-chain as PASS: collateral releases to the agent; a 0.5% witness fee is
   paid from the claims pool pro-rata to attesting witnesses.
5. BondScore updates: `+1` verified success.

### Failure path (slash)

1–3. As above, but quorum majority attest FAIL.
4. The bond resolves on-chain as FAIL; collateral is slashed:
   - 70% to the counterparty (the party who lost because the agent failed),
   - 20% to honest witnesses pro-rata,
   - 10% burned.
5. BondScore updates: `−2` verified failure.

### Dispute path (arbitration)

1. A counterparty publishes kind 37003 (Claim), **gift-wrapped via NIP-59**
   (outer kind 1059, sealed inner kind 37003) so the dispute stays private.
2. An arbitrator publishes kind 37004 (Verdict): `uphold` (treat as fail),
   `reject` (treat as pass), or `split` (partial).
3. On-chain settlement follows the verdict; the arbitrator earns 5% of the
   resolved claim value; BondScore updates accordingly (`uphold` → −2,
   `reject` → +1, `split` → −2).

### Cancellation

Before acceptance, the agent may publish kind 37005; the bond is void, no
collateral is staked.

## BondScore (reputation)

BondScore is a 0–100 scalar per pubkey, published as NIP-32 label events
(kind 1985) in the `bondhive` namespace:

```json
{
  "kind": 1985,
  "tags": [
    ["L", "bondhive"],
    ["l", "bondscore", "<subject pubkey>"],
    ["p", "<subject pubkey>"]
  ],
  "content": "72.4"
}
```

Metrics: `bondscore`, `bondcount`, `bondsuccess`, `bondfail`, `bondfraud`.

Math: start 50; `+1` per verified success, `−2` per verified failure,
`−5` per proven fraud; mean-reverts toward 50 with a 180-day half-life so
stale history fades. The score drives the premium discount curve:
`>= 60 → 10%`, `>= 70 → 25%`, `>= 80 → 50%`.

## Relay requirements

- Kinds 37000 is parameterized-replaceable (30000–39999 range): relays SHOULD
  store only the latest event per `(pubkey, kind, d)`.
- Relays SHOULD index single-letter tags (`a`, `h`, `p`, `v`, `witness`) so
  `REQ {"kinds":[37002], "#a": [addr]}` returns the full attestation set.
- Relays SHOULD honor `h`-tag channel scoping (NIP-29) so bonds stay inside
  their community.
- No special relay behavior is required for kinds 37001–37005 beyond the
  standard NIP-01 storage rules.

## Client behavior

- A client SHOULD verify event signatures (NIP-01) and confirm an attestation
  author is on the bond's witness roster before counting it toward quorum.
- A client SHOULD publish kind 37003 claims only inside NIP-59 gift wrap.
- A client MAY resolve a bond on-chain only after quorum is met; the on-chain
  program independently enforces the same quorum rule.
- Clients MUST NOT treat kind 37001–37005 as anything other than what this
  NIP defines.

## Security

- **Collateral custody**: collateral lives in the Solana program's escrow;
  slashing and release are enforced by the program, not by clients.
- **Sybil / witness collusion**: witnesses are chosen by the bond creator and
  must stake BOND in the mutual pool; a witness who attests falsely forfeits
  that stake (failed-action burn).
- **Gift wrap**: kind 37003 must be wrapped (NIP-59) so disputes are private
  until the verdict is public.
- **Deadline**: an accepted bond whose deadline passes without quorum resolves
  as FAIL by default; clients and the program both enforce this.
- **Rate limiting**: BondScore discount prevents a spammer from buying a
  perfect reputation; each verified outcome requires real attested work.

## Appendix — example events

Service Bond (kind 37000):

```json
{
  "kind": 37000,
  "pubkey": "e5272de914bd3017550f4aa936b470f6eaa3658d2b4c4d6a3f4e6c8d9a0b1c2d",
  "created_at": 1700000000,
  "tags": [
    ["d", "9f3a7b2c1d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a"],
    ["p", "e5272de914bd3017550f4aa936b470f6eaa3658d2b4c4d6a3f4e6c8d9a0b1c2d"],
    ["h", "engineering"],
    ["t", "code-review"],
    ["amount", "10000000000000"],
    ["asset", "BOND"],
    ["chain", "solana"],
    ["deadline", "1700086400"],
    ["witness", "1a2b3c4d5e6f708192a3b4c5d6e7f8091a2b3c4d5e6f708192a3b4c5d6e7f80"],
    ["witness", "2b3c4d5e6f708192a3b4c5d6e7f8091a2b3c4d5e6f708192a3b4c5d6e7f90"]
  ],
  "content": "{\"task\":\"code-review\",\"description\":\"Review PR #42 in the Buzz relay\",\"amount\":\"10000000000000\",\"asset\":\"BOND\",\"chain\":\"solana\",\"deadline\":1700086400,\"created_by_role\":\"agent\"}"
}
```

Performance Attestation (kind 37002):

```json
{
  "kind": 37002,
  "pubkey": "1a2b3c4d5e6f708192a3b4c5d6e7f8091a2b3c4d5e6f708192a3b4c5d6e7f80",
  "created_at": 1700050000,
  "tags": [
    ["a", "37000:e5272de914bd3017550f4aa936b470f6eaa3658d2b4c4d6a3f4e6c8d9a0b1c2d:9f3a7b2c1d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a"],
    ["p", "1a2b3c4d5e6f708192a3b4c5d6e7f8091a2b3c4d5e6f708192a3b4c5d6e7f80"],
    ["v", "pass"],
    ["evidence", "https://github.com/block/buzz/pull/42"]
  ],
  "content": "Attestation: pass"
}
```
