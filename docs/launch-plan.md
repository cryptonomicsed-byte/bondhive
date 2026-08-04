# Bondhive — Launch Plan ($BOND)

**Status:** Design spec, v0.9 (planned — not live). This document is the operational launch plan for the Bondhive protocol and its $BOND token. **It is a spec for a launchable design, not evidence of a live deployment**: no BOND is issued, no curve is open, and no liquidity is deployed as of this writing. Per repo convention, all statuses remain `planned` until verified on-chain.

**Launch principles.**

1. No presale, no private round. 50% of supply (500M BOND) goes through the public pump.fun bonding curve.
2. Mint authority is revoked at migration — supply is fixed at 1B forever.
3. LP tokens are locked 12 months (3-month cliff, then linear); team tokens are locked (24-month vest, 6-month cliff).
4. Every numeric default below is the launch default for a governed parameter, consistent with `docs/tokenomics.md` (v0.9). Cross-reference both documents; tokenomics.md is authoritative on economics.

**Timeline convention.** T0 = the moment the bonding curve opens. All times relative to T0; estimates are marked as such.

## 1. Pre-launch checklist

| Phase | Window | Exit criteria |
|---|---|---|
| Program audit | T−60 → T−14 | Two independent audits: (a) full protocol audit (stake/slash/claims/buy-back programs), (b) focused audit of slash-split, burn, and claims-payout math. Zero critical or high findings at launch; reports published. |
| NIP-74 spec freeze | T−45 | NIP-74 v1.0 frozen: kinds 37000–37005 (bond creation, attestation, dispute, claim, resolution) with required `h`/`d` tags, test vectors, and the machine-readable receipt extension (37006) specified. Registered in the Buzz kind registry and relay ingest gate before any live event is posted. |
| Oracle network bootstrap | T−30 → T−7 | ≥ 7 independent oracle keepers live, each with staked bond collateral; 14-day testnet soak; deviation tolerance 0.5%; 1h heartbeat; keeper stake slashing rules exercised in test. |
| Testnet end-to-end | T−21 → T−7 | Full lifecycle exercised on devnet: create bond → witness attestation → resolve → success release; failure → slash → 70/20/10 split → burn; buy-back execution → 50/50 split; airdrop claim flow; governance vote + timelock. Host-logic tests green (`cargo test -p bondhive-program`). |
| Airdrop snapshot | T−14 | Nostr / Buzz activity snapshot locked at a block height; sybil filter runs (npub age, activity, cluster graph); snapshot hash published. |
| Ops readiness | T−7 | Dashboards live; keeper infrastructure deployed; monitoring + alerting; runbook (this document) distributed; emergency 2-of-3 multisig exercised with a test pause. |
| Legal / comms | T−7 | Docs, terms, and status pages published; language reviewed against the "no presale / utility token / not investment advice" framing of tokenomics.md §11. |

**Hard gate:** launch does not proceed unless all rows above have green exit criteria.

## 2. pump.fun bonding curve launch sequence

### 2.1 Deployment (T−3)

1. Deploy BOND mint (SPL, 9 decimals) and the curve program; verify source.
2. Fund the curve contract with the full 500M BOND tranche (50% of supply).
3. Set curve parameters: opening price, price slope, completion threshold (launch-committed; fixed at migration).
4. Revoke the curve contract's mint authority; confirm on-chain.

### 2.2 Buy mechanics (T0 onward)

- Continuous purchase on the rising bonding curve; buyers receive BOND immediately; no vesting on curve purchases.
- **Anti-bot window:** for the first 6 hours, per-wallet purchase cap of 1% of the tranche (5M BOND) per 10-minute window (launch default, governed).
- Only the public curve exists: team, treasury, and reserve wallets never purchase on or receive from the curve.

### 2.3 Completion

- When the curve fills, purchases halt automatically and a final state is snapshotted. Migration begins within 24h of completion.
- If the curve has not filled by T+30, the launch is paused by governance vote (extend window, or refund and reopen with adjusted parameters — both require a vote; refunds are pro-rata).

### 2.4 Launch timeline

| Time | Event |
|---|---|
| T−60 | Audit kickoff |
| T−45 | NIP-74 v1.0 spec freeze |
| T−30 | Oracle bootstrap complete (≥ 7 keepers) |
| T−21 | Testnet E2E complete |
| T−14 | Airdrop snapshot locked |
| T−7 | Ops readiness gate; docs published |
| T−3 | Programs deployed; mint authority revoked |
| **T0** | **Curve opens — 500M BOND tranche** |
| T0 + 6h | Anti-bot window ends |
| Curve fill (est. T0+1d to T0+7d) | Migration trigger; buys halt |
| Fill + 24h | DEX migration executes |
| Fill + 48h | LP lock deployed (12 months) |
| T+7 | Airdrop claims open |
| T+187 | Airdrop window closes; unclaimed reverts to treasury |
| 1M / 10M / 100M cumulative volume | Milestone burn votes open |

## 3. DEX migration + LP lock

**Migration (fill + 24h).** On curve completion, the raised SOL and the curve's residual BOND are deposited into a Raydium-style CPMM pool. Initial pool price is set to the curve's terminal price, cross-checked against the oracle mid-price; a deviation > 0.5% halts migration for manual review. The migration executes as a single atomic transaction to remove the front-running window.

**LP lock (fill + 48h).** LP tokens are minted to the protocol and immediately deposited into a verified lock contract: **12-month total lock, 3-month cliff, then linear unlock of 1/9 per month (months 4–12)**. The lock contract has no early-exit function. Post-migration, mint and freeze authorities are revoked (renounced) — supply is permanently fixed at 1,000,000,000 BOND.

**Post-migration verification checklist (published tx hashes for each):** pool address and initial price; LP lock deposit; mint-authority revocation; burn-address setup; terminal curve price vs pool price delta.

## 4. Airdrop mechanics

**Reserve:** 100M BOND (10% of supply).

**Snapshot (T−14):** Nostr activity plus early Buzz usage, keyed by npub (public key), not wallet. Eligibility and weights are computed off-chain from the snapshot; the snapshot hash is published at T−14.

**Eligibility (governed defaults):**

| Criterion | Default |
|---|---|
| npub age | Created ≥ 90 days before snapshot |
| Activity | ≥ 10 activity events in the snapshot window (posts, reactions, follows) |
| Sybil graph check | Cluster detection; duplicate/farm clusters excluded; no two claimed npubs share a cluster |
| Per-npub cap | 0.1% of reserve (100,000 BOND) |

**Allocation:** quadratic — allocation ∝ √(activity), normalized so the reserve sums to exactly 100M BOND. This flattens farmed activity while preserving meaningful weight for genuine early users.

**Claim flow (identity proof, nsec never leaves the client):**

1. User opens the claim page; the client generates a challenge.
2. The user signs the challenge with their nsec (NIP-19) in the client.
3. The client submits `{npub, signature, Solana address}` — the nsec itself is never transmitted and never touches the server.
4. The server verifies the signature against the npub's public key, checks eligibility, and disburses from the reserve to the Solana address.
5. Claim fee: 0.0001 SOL (spam deterrent, governed), refunded if the claim is rejected due to a protocol error.

**Deadlines:** claims open T+7, close T+187 (180-day window). Unclaimed tokens revert to the protocol treasury, where governance may vote to burn them.

## 5. Buy-back + burn operating runbook

**Keeper network:** ≥ 3 independent keepers (decentralized execution): one executes, the others verify every transaction. Verification is a signed log entry; a keeper caught reporting false executions is slashed from its oracle bond.

**Cadence & triggers:**

| Parameter | Default |
|---|---|
| Check interval | Every 6 hours |
| Execution trigger | Treasury BOND-equivalent ≥ max(0.1% of trailing 24h DEX volume, 1,000 USD-eq) |
| Minimum frequency | At least weekly if any treasury balance exists |

**Execution rules:**

| Rule | Default |
|---|---|
| Venue | Primary DEX pool; aggregator fallback |
| TWAP | ≤ 60 minutes per execution |
| Per-order price impact | ≤ 1% |
| Daily purchase | ≤ 2% of pool depth |
| Hard slippage cap | 1.5% |
| Oracle sanity | Halt execution if oracle mid-price deviates > 5% at execution time |

**Post-trade disposition (in order):** 0.1% of notional to oracle keepers → 50% burned via SPL burn instruction (tx published) → 50% redistributed pro-rata to BOND stakers, snapshotted at the execution block (direct transfer ≤ 10k stakers; merkle claims beyond).

**Reporting:** every execution publishes notional, average price, price impact, fees, burned amount, redistributed amount. A monthly report reconciles all executions against treasury inflows, with a cumulative-burn dashboard per burn type (slash-burn / buy-back burn / milestone burn / failed-action burn).

**Circuit breakers:** pause on (a) oracle deviation > 5%, (b) pool-manipulation flag, (c) governance pause vote. Resume by governance vote, or automatic 24h resume after fresh data confirms conditions cleared.

## 6. Milestone burn schedule

| Cumulative bond volume | Burn | Gate | Window |
|---|---|---|---|
| 1M BOND | 0.5% of then-circulating supply | Governance vote: majority of votes cast, quorum 10% of staked supply | Vote opens within 7 days of crossing; executes within 72h of passage |
| 10M BOND | 0.5% of then-circulating supply | Same | Same |
| 100M BOND | 0.5% of then-circulating supply | Same | Same |

The default proposal at each threshold is "burn"; abstentions count against passage, so a silent community does not trigger a burn by default. Thresholds are governed parameters — raising a threshold requires a vote, and any threshold change applies only to future milestones. Burn execution is a programmatic SPL burn of circulating-supply tokens held in a designated milestone-burn escrow funded by governance at each step.

## 7. Post-launch growth loops

| Loop | Mechanism | Owner | Primary KPI |
|---|---|---|---|
| Buzz integration | Service Bonds render natively in Buzz feeds as NIP-74 events; bond-verified badges on agent profiles; "create bond" from Buzz UI via the Bondhive SDK (kinds registered in the relay ingest gate, per AGENTS.md). | buzz/ | Bonds created from Buzz UI |
| MCP distribution | Bondhive MCP server exposes `bond_create`, attestation status, and BondScore queries to agent frameworks; tools emit Nostr events, never bespoke HTTP endpoints. Every agent that can call tools is a potential bond creator — the SDK is the growth channel. | buzz/mcp | MCP installs → bonds created |
| Witness marketplace | BOND stakers sign up as witnesses; yield = per-attestation fees + 20% slash share + pool surplus; onboarding via testnet faucet and docs. Target ≥ 100 active witnesses by T+30 (target, not commitment). | program/ | Active witnesses |
| Data flywheel | Attestation history → BondScore → premium discounts (§8.2 of tokenomics.md) → cheaper bonds → more volume → more attestation history. The flywheel is the moat; fraud erodes it symmetrically. | oracle/ | BondScore coverage ≥ 60 share |

**Sequencing:** Buzz integration ships first (T+0, it is the demand side), MCP second (T+14, it is the distribution side), witness marketplace continuously (supply side). No growth loop depends on undisclosed partners — all three are built on protocol-owned surfaces.

## 8. KPI dashboard definitions

All KPIs are computed from on-chain state + published keeper logs. Weekly ops report; monthly public report.

| KPI | Definition | Source | Year-1 target |
|---|---|---|---|
| Cumulative bond volume (BOND) | Σ bond values, resolved or pending | On-chain | ≥ 10M |
| Bonds created / day | Count of kind 37000 events | Relay + chain | ≥ 30/day avg |
| Success rate | Verified successes ÷ resolved bonds | On-chain | ≥ 95% |
| Average premium | Σ premiums ÷ Σ bond values | On-chain | 2.0–2.5% |
| Active witnesses | Witnesses with ≥ 1 attestation in 30d | On-chain | ≥ 100 |
| Witness fraud rate | Proven fraudulent attestations ÷ attestations | On-chain | < 0.1% |
| Staked BOND | Mutual-pool stake | On-chain | ≥ 30% of circulating |
| Buy-backs executed / BOND burned | By burn taxonomy (§5 of tokenomics.md) | Keeper logs + chain | Reported, not targeted |
| Airdrop claim rate | Claimed ÷ eligible, by T+60 | Claim contract | ≥ 50% |
| Sybil rejected | Accounts filtered at snapshot + claim | Snapshot pipeline | Reported |
| DEX liquidity / depth | Pool reserves + 1% depth | Pool | Reported |
| Claims paid / coverage ratio | Payouts ÷ pool balance; pool ÷ 30-day claims | On-chain | Pool ≥ 3× 30-day claims |
| Governance turnout | Votes ÷ staked supply | Governance program | ≥ 10% avg |
| Agent funnel | Agents with BondScore ≥ 60; bonds per active agent | On-chain | Reported |
| LP lock status | Locked/unlocked amounts, next unlock date | Lock contract | 100% locked until month 4 |

**Definition of done for launch:** every item in the §1 checklist green; curve open and filling; migration + LP lock verified on-chain; airdrop claims live at T+7; keeper buy-backs operating by T+14; first weekly ops report published. All of the above is a plan, not a record — this document will be updated in place as each milestone is actually reached, with statuses flipped from `planned` to `live` only on verified on-chain evidence.
