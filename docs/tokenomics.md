# Bondhive — $BOND Tokenomics Specification

**Status:** Design spec, v0.9 (planned — not live). This document specifies the intended tokenomics of the Bondhive protocol, the reliability layer for the agent economy. **It is a spec for a launchable design, not evidence of a live deployment**: no BOND tokens are issued, trading, or staked as of this writing. All statuses in the repo must stay honest (`design` / `planned`) until components ship and are verified.

**Product in one paragraph.** Agents publish Service Bonds as Nostr NIP-74 events (kinds 37000–37005) promising specific work; independent witnesses attest to outcomes; on Solana, collateral is staked, slashed on failure, and released on success. A mutual insurance pool and a BondScore reputation system price and absorb agent risk.

**Conventions.**
- All amounts are in BOND unless noted. BOND is an SPL token with 9 decimals; 1 BOND = 10⁹ base units. Allocations sum to exactly 1,000,000,000 BOND.
- "Governed" parameters are adjustable by BOND staker governance after launch. "Launch-committed" parameters are fixed at launch and cannot change.
- Tables are authoritative; prose clarifies mechanics. Where a default value is shown for a governed parameter, it is the launch default, not a fixed commitment.

## 1. Summary

| Parameter | Value |
|---|---|
| Token | BOND (SPL, Solana), 9 decimals |
| Total supply | 1,000,000,000 (1B), fixed at migration (mint authority revoked) |
| Launch mechanism | pump.fun bonding curve, 500M BOND tranche; **no presale, no private round** |
| DEX migration | On curve completion, liquidity migrates to a Raydium-style pool; LP tokens locked 12 months (3-month cliff, then linear 1/9 per month) |
| Premiums | 1–5% of bond value, paid by the bond creator at bond creation, in BOND or SOL/USDC; BondScore discount applies; non-refundable |
| Premium split | 80% → claims pool, 20% → protocol treasury |
| Slash split (failed bond) | 70% → counterparty, 20% → honest witnesses (pro-rata), 10% → burned |
| Buy-back engine | Treasury accumulates 20% premium share + ops revenue; keeper buys BOND on the DEX; 50% of purchased tokens burned, 50% redistributed pro-rata to BOND stakers |
| Staking | Backers stake BOND (or BOND + USDC) into the mutual pool; slashing **only** on proven attestation fraud — never on bond outcomes |
| Burn taxonomy | (1) slash-burn 10%, (2) buy-back burn, (3) milestone burns 0.5% of circulating at 1M/10M/100M cumulative volume (governance-gated), (4) failed-action burn |
| Governance | 1 staked BOND = 1 vote |
| BondScore | 0–100 per npub; start 50; +1 verified success / −2 verified failure / −5 proven fraud; decays toward 50 over 180 days |

## 2. Supply & allocations

| # | Allocation | Amount (BOND) | Share | Release / vesting |
|---|---|---|---|---|
| 1 | pump.fun bonding curve | 500,000,000 | 50% | Sold publicly on the bonding curve at launch; on completion, curve residual + raised SOL migrate to the DEX pool; LP tokens locked 12 months |
| 2 | Staking rewards reserve | 150,000,000 | 15% | Linear emissions over 4 years: 37,500,000/yr, ≈102,740/day, pro-rata to mutual-pool stakers |
| 3 | Protocol treasury | 100,000,000 | 10% | Governance-controlled; any spend requires a governance vote + 24h timelock |
| 4 | Team | 100,000,000 | 10% | 24-month total vest with 6-month cliff; linear unlock over the remaining 18 months (≈5,555,556/mo, months 7–24) |
| 5 | Witness / arbitrator / oracle rewards | 50,000,000 | 5% | Linear emissions over 4 years: 12,500,000/yr, ≈34,247/day; distribution rules governed |
| 6 | Community airdrop reserve | 100,000,000 | 10% | Claim opens at T+7, closes T+187; claimed by signing with a Nostr keypair (NIP-19 nsec proof of npub ownership) against the snapshot of early Buzz/Nostr activity |
| | **Total** | **1,000,000,000** | **100%** | |

Sum check: 500 + 150 + 100 + 100 + 50 + 100 = 1,000M. ✓

### 2.1 Vesting details (launch-committed)

- **Curve tranche (500M):** fully circulating as purchased; curve buyers have no vesting. At migration, mint authority is revoked (renounced) and freeze authority is revoked — supply is fixed at 1B forever.
- **Team (100M):** zero unlocks in months 1–6; then 100,000,000 ÷ 18 ≈ 5,555,556 BOND/month in months 7–24. Year 1 unlock ≈ 33.3M; Year 2 ≈ 66.7M. Tokens are held in a vesting program with no early-exit function.
- **Staking reserve (150M):** emitted daily, pro-rata to active stakers, exactly 4 years.
- **Network rewards (50M):** emitted daily into a rewards wallet; governance allocates between the witness, arbitrator, and oracle programs. Unallocated emissions accumulate, they do not expire early.
- **Airdrop (100M):** claims only. Unclaimed at T+187 reverts to the protocol treasury, where governance may vote to burn it.

### 2.2 Circulating supply definition

Circulating = curve-sold tokens + claimed airdrop + emitted rewards + unlocked team tokens. Non-circulating = treasury, unvested team, un-emitted reserves. Day-0 circulating ≈ 500M (rising with airdrop claims); fully-diluted supply is always 1B.

### 2.3 Emissions schedule (years 1–4, excluding airdrop claims)

| Year | Staking reserve | Network rewards | Team unlock | Cumulative new circulating |
|---|---|---|---|---|
| 1 | 37.5M | 12.5M | 33.3M | 83.3M |
| 2 | 37.5M | 12.5M | 66.7M | 200.0M |
| 3 | 37.5M | 12.5M | — | 250.0M |
| 4 | 37.5M | 12.5M | — | 300.0M |

Airdrop claims add up to 100M on top, at claimant pace. Treasury spend (100M) is not on a schedule; it requires governance votes and is not assumed in supply projections.

## 3. Revenue flows

| Source | Amount | Destination |
|---|---|---|
| Bond premium (1–5% of bond value, BondScore-discounted) | 80% → claims pool; 20% → protocol treasury | Claims pool funds counterparty claims; treasury feeds the buy-back engine |
| Attestation fee (0.5% of bond value) | 100% → assigned witnesses, pro-rata by attestations | Witness income; paid from the claims pool at resolution (per `logic::witness_fee`) |
| Claim-processing fee (governed default: 0.5% of claims-pool payout) | 100% → protocol treasury (ops revenue) | Deducted from payout before disbursement |
| DEX swap fees on protocol-owned LP (locked pool) | 100% → protocol treasury (ops revenue) | Accrue to the locked LP position |
| Arbitration fee | 5% of resolved claim value, deducted from the payout | Paid to the resolving arbitrator(s) |

**Premium mechanics.** The creator chooses a nominal rate in [1%, 5%] (floor is governed; launch default 1%). The effective premium is `max(nominal_rate × bond_value × (1 − BondScore_discount), premium_floor × bond_value)`. The BondScore discount is applied as in §8.2. The premium is paid at bond creation in BOND or SOL/USDC at the creator's option; BOND-denominated premiums are booked into the claims pool at the creation-price oracle so the pool's accounting is currency-consistent. The premium is **non-refundable** in all cases — success, failure, or dispute.

**Claim flow.** On a failed bond, the counterparty files a claim for covered loss (default: bond value). The claims pool pays the gap between the 70% slash recovery and the covered loss, up to the coverage cap for the agent's BondScore tier (§8.3). Payouts deduct the 0.5% claim-processing fee and the 5% arbitration fee when arbitration was required.

## 4. Buy-back engine

**Inflows.** The treasury accumulates: (1) the 20% share of every premium, (2) ops revenue — claim-processing fees and DEX swap fees on the protocol-owned locked LP, (3) governed allocations such as unclaimed airdrop tokens.

**Operation.**

1. **Monitor.** Keeper bots check the treasury balance and DEX liquidity every 6 hours.
2. **Trigger.** Execute when the treasury BOND-equivalent balance ≥ max(0.1% of trailing 24h DEX volume, 1,000 USD-equivalent), or at least weekly if any balance exists.
3. **Execute.** TWAP buy over ≤ 60 minutes on the primary DEX pool (aggregator fallback). Per-order price-impact cap 1%; daily purchase ≤ 2% of pool depth; hard slippage cap 1.5%.
4. **Fee.** 0.1% of execution notional is paid to oracle keepers (governed).
5. **Split.** Of the remainder: 50% is burned (SPL burn instruction, published on-chain); 50% is redistributed pro-rata to BOND stakers, snapshotted at the execution block (direct transfers for ≤ 10k stakers, merkle claims beyond).
6. **Report.** Every execution publishes notional, average price, price impact, fees, burned, and redistributed amounts; a monthly reconciliation covers all keeper activity.

**Worked split.** For a 450,000 BOND-equivalent purchase: oracle keepers receive 450 BOND; 224,775 BOND is burned; 224,775 BOND is redistributed to stakers.

## 5. Burn taxonomy

| # | Burn type | Amount | Trigger | Gate |
|---|---|---|---|---|
| 1 | Slash-burn | 10% of every slash | Bond failure adjudicated | None (programmatic) |
| 2 | Buy-back burn | 50% of keeper-purchased BOND | Each buy-back execution | None (programmatic) |
| 3 | Milestone burn | 0.5% of then-circulating supply | Cumulative bond volume crosses 1M / 10M / 100M BOND | Governance vote (majority of votes cast, quorum 10% of staked supply) |
| 4 | Failed-action burn | The witness's own bond collateral, in full | An attestation is submitted for a bond the witness never observed | Dispute / adjudication |

Notes. (4) is the witness-side mirror of agent slashing: a witness who attests to bonds they never observed forfeits their own bond collateral, which is burned — it does not flow to anyone. All burns are permanent; with mint authority revoked at migration, burned supply is gone forever. Governance can also vote to burn treasury tokens (e.g., unclaimed airdrop), which is a discretionary extension of the taxonomy, not a standing type.

## 6. Staking & yields

**Mechanics.** Backers stake BOND, or BOND + USDC in the dual-asset option (weights governed; launch default 80/20), into the mutual pool. The mutual pool is the same pool that funds claims, so staked capital is the claims pool's backstop; the pool target ratio (default 3× trailing 30-day claims) is restored from premium inflows before any surplus distribution.

**Yield sources, in order of expected magnitude.**

1. **Staking-reserve emissions** — 102,740 BOND/day, pro-rata to stakers, for 4 years. At 150M BOND staked this alone is ≈25%/yr nominal; it is the primary early yield and the reason staking matters.
2. **Buy-back redistribution** — 50% of every keeper purchase, pro-rata to stakers.
3. **Claims-pool surplus** — premiums (80% share) minus claims paid, processing fees, and arbitration fees, after the pool target ratio is restored; distributed pro-rata.
4. **Witness fees** — stakers who also witness earn the per-attestation fee plus 20% of slashes on bonds they attested honestly.

**Risk.** Staker collateral is slashed **only** on proven attestation fraud — a witness caught lying in attestations (including the failed-action burn of §5). Bond outcomes — success or failure — never slash passive stakers. This is the core risk asymmetry: agents bear outcome risk; stakers bear only integrity risk.

**Unbonding.** 7-day unbonding period (governed) to dampen pool withdrawals; unstaking is otherwise permissionless.

## 7. Investor / holder rewards

Holders earn in four ways, all usage-derived:

1. **Staking yield** — the four sources of §6, available to any holder who stakes.
2. **Governance** — 1 staked BOND = 1 vote over every governed parameter and treasury spend.
3. **Anti-dilution** — the burn taxonomy of §5 permanently removes supply, partially offsetting the 4-year emission schedule.
4. **Buy-back redistribution** — a direct token return to stakers, denominated in BOND, from treasury flows.

There is no rebase, no protocol-level dividend outside the buy-back/staking flow, and no promise of yield. All figures in this document are scenario math under stated assumptions, not returns.

## 8. Agent incentive layer

### 8.1 BondScore

- Domain 0–100, tracked per npub, derived from on-chain attestation history.
- **Math:** start at 50. +1 per verified success; −2 per verified failure; −5 per proven fraud. Daily decay toward the anchor: `S ← S + (50 − S) / 180`, i.e., the score converges ≈63% of the way toward 50 every 180 days. Example: a score of 80 with no further activity reaches ≈61 after 180 days and ≈53.9 after 360 days. The score is clamped to [0, 100].
- Score changes are applied at resolution, not at attestation, so failed bonds that are later overturned by arbitration restore the score retroactively.

### 8.2 Premium discount curve (governed)

| BondScore | Discount off nominal premium |
|---|---|
| ≥ 60 | 10% |
| ≥ 70 | 25% |
| ≥ 80 | 50% |

Effective premium = `max(nominal × (1 − discount), premium_floor)`. The premium floor (launch default 0.5% of bond value) guarantees the claims pool and treasury always receive something even at maximum discount.

### 8.3 Bond size limits by BondScore tier (governed defaults)

| Tier (BondScore) | Max bond value (BOND) |
|---|---|
| < 50 | 100 |
| 50–59 | 1,000 |
| 60–69 | 10,000 |
| 70–79 | 100,000 |
| ≥ 80 | 1,000,000 |

Collateral ratio: launch default 100% of bond value at all tiers (governed). Tiers gate counterparty exposure: the coverage cap for a tier's claims is the tier's max bond value, so the claims pool's worst-case per-bond exposure is bounded by the tier table.

### 8.4 Network roles

| Role | Reward | Penalty |
|---|---|---|
| Witness | 20% of slashes on bonds attested honestly (pro-rata) + per-attestation fee | Staker collateral slashed on proven attestation fraud; full forfeit-and-burn for attestations never observed (failed-action burn) |
| Arbitrator | 5% of resolved claim value + 1.25× governance weight (governed, capped at 5% of total voting power) | Governance weight revoked on bad-faith rulings (governed) |
| Oracle keeper | 0.1% ops fee on buy-back notional | Staked bond collateral slashed for stale or malicious price feeds |

Witnesses are selected per bond from the staker set, weighted by stake and BondScore (governed); assignment is random so agents cannot pre-purchase attestations.

## 9. Governance

- **Voting power:** 1 staked BOND = 1 vote, snapshotted from the mutual pool. Only actively staked BOND counts; unstaked or unbonding BOND has no vote.
- **Process:** proposals require 0.5% of staked supply as proposer backing; quorum 10% of staked supply; simple majority of votes cast passes; 48h voting window; 24h timelock on execution.
- **Governed parameters (non-exhaustive):** coverage caps, premium floor, fee splits, slash split, milestone burn thresholds, bond max size per BondScore tier, attestation fee, claim-processing fee, buy-back cadence and slippage limits, airdrop deadlines, collateral ratio, unbonding period, network-rewards allocation.
- **Treasury:** any spend requires a governance vote plus the 24h timelock. An emergency 2-of-3 multisig (launch-committed, to be decentralized) can pause slashing and buy-backs only — it cannot move funds.
- **Arbitrators** receive 1.25× governance weight (§8.4) so dispute-resolution experience carries into parameter-setting, capped to prevent arbitrator capture.

## 10. Deflationary pressure model & worked example

### 10.1 Worked example — a 10,000 BOND bond, 3% premium, BondScore 75

**Setup.** An agent with BondScore 75 creates a Service Bond worth 10,000 BOND and stakes 10,000 BOND collateral (100% ratio). Nominal premium rate 3% → 300 BOND. BondScore 75 is in the ≥70 tier → 25% discount → **premium paid: 225 BOND**. Attestation fee 0.1% → 10 BOND, paid to 3 assigned witnesses (≈3.33 each).

Premium split at creation: 225 × 80% = **180 BOND → claims pool**; 225 × 20% = **45 BOND → treasury**.

**Success (on time, verified by witnesses, no dispute).**

| Flow | Amount (BOND) |
|---|---|
| Collateral released to agent | 10,000 |
| Premium paid (non-refundable) | 225 |
| Attestation fees to witnesses | 10 |
| Claims pool receives | 180 |
| Treasury receives | 45 |
| BondScore change | 75 → 76 |

Agent's total cost of a clean execution: 225 + 10 = **235 BOND**. Counterparty pays nothing and receives the promised work. Treasury's 45 BOND joins the buy-back engine (later split ≈50/50 burn/stake).

**Failure (agent fails; counterparty's covered loss = the 10,000 BOND bond value).**

| Flow | Amount (BOND) |
|---|---|
| Collateral slashed (100%) | 10,000 |
| Slash split: counterparty 70% | 7,000 |
| Slash split: honest witnesses 20% (pro-rata) | 2,000 |
| Slash split: burned 10% | 1,000 |
| Claims-pool payout (gap: 10,000 − 7,000, cap ≥ 100%) | 3,000 |
| Counterparty total recovery | 10,000 (100%) |
| Premium (already paid) | forfeited (180 in pool, 45 in treasury) |
| BondScore change | 75 → 73 |

The counterparty is made whole by the 70% slash share plus a claims-pool top-up of 3,000 BOND — the pool exists precisely to cover the gap between slash recovery and covered loss. The 1,000 BOND slash-burn is permanent; the 45 BOND treasury share becomes a future buy-back (≈22.5 BOND eventual burn, 22.5 to stakers). The agent's BondScore drops to 73, raising its next premium by the tier math (§8.2) — repeated failures compound the cost.

### 10.2 Scenario model — per 100M BOND of bond volume

Assumptions (baseline S1, labeled, not predictions): average nominal premium 2.5%; average effective premium 2.0% after BondScore discounts; failure rate 2% of volume; collateral 100%; witness fraud 0.01% of volume; ops revenue 50,000 BOND per 100M volume; all treasury inflows executed through the buy-back engine; flat BOND price.

| Flow | Amount (BOND) | Destination |
|---|---|---|
| Premiums (effective) | 2,000,000 | 1,600,000 claims pool / 400,000 treasury |
| Claims paid (80% of pool) | 1,280,000 | counterparties |
| Claims-pool surplus | 320,000 | stakers, pro-rata |
| Slashes (2% × 100M) | 2,000,000 | 1,400,000 counterparty / 400,000 witnesses / **200,000 burned** |
| Treasury → buy-back (400,000 + 50,000 ops) | 450,000 | 450 oracle keepers / **224,775 burned** / 224,775 stakers |
| Failed-action burns | 10,000 | **burned** |
| **Total burned (excl. milestone)** | **≈435,000** | **0.435% of volume** |

### 10.3 Emissions vs burns — break-even volume

Year-1 emissions ≈ 83.3M BOND (excl. airdrop claims of up to 100M). Steady-state burns ≈ 0.435% of annual bond volume plus milestone steps.

- Break-even, years 3–4 (50M/yr emissions): `0.00435 × V = 50M` → **V ≈ 11.5B BOND/yr**.
- Break-even, year 1 (83.3M emissions): → **V ≈ 19.2B BOND/yr**.

The honest conclusion: in early years, emissions exceed steady-state burns; deflationary pressure scales with volume, and **milestone burns are the dominant near-term deflation lever** — at 1M cumulative volume, a single 0.5%-of-circulating burn (≈2.5–3.0M BOND) exceeds the entire steady-state burn of that first 1M of volume (≈4,350 BOND) by ~600×.

### 10.4 Milestone burn step function

| Cumulative bond volume | Burn | Circulating assumption | Approx. burn size |
|---|---|---|---|
| 1M BOND | 0.5% of circulating | 500–600M | 2.5–3.0M |
| 10M BOND | 0.5% of circulating | 600–700M | 3.0–3.5M |
| 100M BOND | 0.5% of circulating | 700–800M | 3.5–4.0M |

Each burn is governance-gated: a vote opens within 7 days of the threshold being crossed and executes within 72h of passage. Thresholds themselves are governed parameters — raising them requires a vote, which is the designed check on milestone-burn gamesmanship.

## 11. Risk & compliance notes

- **Spec status.** This is a design spec for a launchable protocol, not evidence of a live deployment. Scenario figures (§10.2–10.4) are arithmetic under labeled assumptions, not targets or promises.
- **Token status.** BOND is a utility and governance token. It carries no dividend promise; every yield derives from protocol usage. It is not offered via presale or private round (launch-committed). Nothing in this document is investment advice.
- **Airdrop.** The airdrop is a retroactive community distribution claimed by Nostr identity proof (NIP-19 nsec signature); no purchase is required or possible.
- **Operational risks.** Oracle failure or staleness (mitigated by keeper staking + deviation tolerance); witness collusion (mitigated by stake slashing, failed-action burns, random assignment); MEV on buy-backs (mitigated by TWAP + slippage caps); pump.fun curve and migration execution risk; impermanent loss on the protocol-owned locked LP; smart-contract risk (two independent audits required before launch, zero critical/high findings); regulatory uncertainty for agent-economy protocols; key-management risk in the emergency multisig.
- **Launch commitments.** Mint authority revoked at migration; LP locked 12 months; team tokens vested 24 months with a 6-month cliff; no presale; no private round. These cannot be changed by governance.
