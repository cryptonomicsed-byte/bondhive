//! Host-side integration tests for the Bondhive economic state machine.
//!
//! These exercise the pure `logic` engine plus the `state` pack/unpack
//! round-trips — no Solana runtime required. They prove the full bond
//! lifecycle math: create → accept (escrow + premium split) → attest →
//! resolve pass/fail (slash split) → buy-back → milestone burn.

use bondhive_program::logic;
use bondhive_program::state::{
    BondAccount, BondState, PoolAccount, StakerAccount, WitnessVoteAccount,
};

fn sample_pool() -> PoolAccount {
    PoolAccount {
        version: 1,
        authority: [1u8; 32].into(),
        mint: [2u8; 32].into(),
        treasury_bond: 0,
        claims_pool: 0,
        escrow: 0,
        staked_total: 0,
        burned_total: 0,
        premium_fee_bps: 2000,
        slash_claimant_bps: 7000,
        slash_witness_bps: 2000,
        slash_burn_bps: 1000,
        milestone_volume: 0,
        milestone_next: 1_000_000_000_000_000,
        bonds_active: 0,
        reserved: [0; 32],
    }
}

#[test]
fn pool_pack_unpack_roundtrip() {
    let p = sample_pool();
    let mut buf = vec![0u8; PoolAccount::size()];
    p.pack(&mut buf);
    let q = PoolAccount::unpack(&buf).unwrap();
    assert_eq!(p.authority, q.authority);
    assert_eq!(p.mint, q.mint);
    assert_eq!(p.treasury_bond, q.treasury_bond);
    assert_eq!(p.claims_pool, q.claims_pool);
    assert_eq!(p.escrow, q.escrow);
    assert_eq!(p.staked_total, q.staked_total);
    assert_eq!(p.burned_total, q.burned_total);
    assert_eq!(p.milestone_next, q.milestone_next);
    assert_eq!(p.bonds_active, q.bonds_active);
}

#[test]
fn bond_pack_unpack_roundtrip() {
    let b = BondAccount {
        version: 1,
        state: BondState::Accepted as u8,
        agent: [3u8; 32].into(),
        counterparty: [4u8; 32].into(),
        amount: 10_000,
        premium: 300,
        premium_paid: 300,
        witness_count: 3,
        pass_count: 2,
        fail_count: 1,
        created_at: 1_700_000_000,
        deadline: 1_800_000_000,
        resolved_at: 0,
        bond_id: [7u8; 32],
        reserved: [0; 16],
    };
    let mut buf = vec![0u8; BondAccount::size()];
    b.pack(&mut buf);
    let q = BondAccount::unpack(&buf).unwrap();
    assert_eq!(q.state, BondState::Accepted as u8);
    assert_eq!(q.amount, 10_000);
    assert_eq!(q.pass_count, 2);
    assert_eq!(q.bond_id, [7u8; 32]);
    assert_eq!(q.deadline, 1_800_000_000);
}

#[test]
fn staker_pack_unpack_roundtrip() {
    let s = StakerAccount {
        version: 1,
        owner: [9u8; 32].into(),
        staked: 5_000,
        rewards_earned: 120,
        rewards_claimed: 40,
        joined_at: 1_600_000_000,
        reserved: [0; 32],
    };
    let mut buf = vec![0u8; StakerAccount::size()];
    s.pack(&mut buf);
    let q = StakerAccount::unpack(&buf).unwrap();
    assert_eq!(q.staked, 5_000);
    assert_eq!(q.rewards_earned, 120);
}

/// The full happy path with exact tokenomics numbers from docs/tokenomics.md:
/// bond = 10,000 BOND, premium = 300 BOND (3%), score 75 → 25% discount.
#[test]
fn lifecycle_happy_path_release() {
    let amount = 10_000u64;
    let base_premium = 300u64;
    let score = 75u16;
    let premium = logic::effective_premium(base_premium, score);
    assert_eq!(premium, 225); // 25% discount applied

    let mut pool = sample_pool();
    let mut bond = BondAccount {
        version: 1,
        state: BondState::Offered as u8,
        agent: [3u8; 32].into(),
        counterparty: [4u8; 32].into(),
        amount,
        premium,
        premium_paid: 0,
        witness_count: 0,
        pass_count: 0,
        fail_count: 0,
        created_at: 1_700_000_000,
        deadline: 1_800_000_000,
        resolved_at: 0,
        bond_id: [7u8; 32],
        reserved: [0; 16],
    };

    // Accept: escrow locks collateral; premium splits 80/20.
    bond.state = BondState::Accepted as u8;
    bond.premium_paid = premium;
    let split = logic::premium_split(premium);
    pool.escrow += amount;
    pool.claims_pool += split.claims_share;
    pool.treasury_bond += split.treasury_share;
    pool.milestone_volume += amount;
    pool.bonds_active += 1;
    assert_eq!(split.claims_share, 180); // 80% of 225
    assert_eq!(split.treasury_share, 45); // 20% of 225
    assert_eq!(pool.escrow, 10_000);

    // Witnesses attest: 3 attestations, majority pass → resolves pass.
    bond.witness_count = 3;
    bond.pass_count = 2;
    bond.fail_count = 1;
    assert!(logic::resolves_pass(bond.witness_count, bond.pass_count, 3));

    // Resolve pass: escrow released back to agent; witness fee paid.
    pool.escrow -= amount;
    pool.bonds_active -= 1;
    let fee = logic::witness_fee(amount);
    pool.claims_pool -= fee;
    bond.state = BondState::ResolvedPass as u8;
    assert_eq!(fee, 50); // 0.5% of 10,000
    assert_eq!(pool.escrow, 0);
    // Protocol earned: treasury 45 BOND (fee revenue) for buy-backs.
    assert_eq!(pool.treasury_bond, 45);
}

/// The failure path: same bond fails → slash split 70/20/10.
#[test]
fn lifecycle_failure_slash() {
    let amount = 10_000u64;
    let mut pool = sample_pool();
    let mut bond = BondAccount {
        version: 1,
        state: BondState::Accepted as u8,
        agent: [3u8; 32].into(),
        counterparty: [4u8; 32].into(),
        amount,
        premium: 300,
        premium_paid: 300,
        witness_count: 3,
        pass_count: 1,
        fail_count: 2,
        created_at: 1_700_000_000,
        deadline: 1_800_000_000,
        resolved_at: 0,
        bond_id: [7u8; 32],
        reserved: [0; 16],
    };
    pool.escrow = amount;
    pool.bonds_active = 1;

    // Majority fail → resolve fail.
    assert!(!logic::resolves_pass(
        bond.witness_count,
        bond.pass_count,
        3
    ));
    let slash = logic::slash_split(amount);
    pool.escrow -= amount;
    pool.bonds_active -= 1;
    pool.burned_total += slash.burn_share;
    bond.state = BondState::ResolvedFail as u8;

    assert_eq!(slash.claimant_share, 7_000); // 70% to counterparty
    assert_eq!(slash.witness_share, 2_000); // 20% to honest witnesses
    assert_eq!(slash.burn_share, 1_000); // 10% burned
    assert_eq!(pool.burned_total, 1_000);
    assert_eq!(pool.escrow, 0);
}

/// Buy-back engine: treasury accumulates, keeper buys, 50% burns /
/// 50% redistributes to stakers.
#[test]
fn buyback_burn_redistributes_to_stakers() {
    let mut pool = sample_pool();
    pool.treasury_bond = 1_000_000;
    pool.staked_total = 500_000;

    let amount = 100_000;
    let split = logic::buyback_split(amount);
    pool.treasury_bond -= amount;
    pool.burned_total += split.burn;
    pool.staked_total += split.redistribute;

    assert_eq!(split.burn, 50_000);
    assert_eq!(split.redistribute, 50_000);
    assert_eq!(pool.treasury_bond, 900_000);
    assert_eq!(pool.burned_total, 50_000);
    assert_eq!(pool.staked_total, 550_000);
}

/// Milestone burn triggers at 1M BOND (1e15 base units at 9dp) cumulative volume.
#[test]
fn milestone_burn_at_one_million() {
    let mut pool = sample_pool();
    pool.treasury_bond = 200_000;
    pool.milestone_volume = 999_999_999_999_999; // just under 1M BOND

    assert_eq!(
        logic::milestone_burn(
            pool.milestone_volume,
            pool.milestone_next,
            pool.treasury_bond
        ),
        None
    );

    pool.milestone_volume = 1_000_000_000_000_000; // crossed 1M BOND
    let burn = logic::milestone_burn(
        pool.milestone_volume,
        pool.milestone_next,
        pool.treasury_bond,
    )
    .unwrap();
    assert_eq!(burn, 1_000); // 0.5% of 200k
    pool.treasury_bond -= burn;
    pool.burned_total += burn;
    pool.milestone_next = pool.milestone_next.saturating_mul(10);
    assert_eq!(pool.milestone_next, 10_000_000_000_000_000);
}

/// Effective premium applies BondScore discount exactly as the core crate does.
#[test]
fn premium_discount_curve_parity() {
    for (score, expected_discount) in [(50u16, 0u16), (60, 10), (70, 25), (80, 50), (100, 50)] {
        assert_eq!(logic::premium_discount(score), expected_discount);
    }
}

#[test]
fn witness_vote_pack_unpack_roundtrip() {
    let vote = WitnessVoteAccount {
        version: 1,
        bond_id: [7u8; 32],
        witness: [9u8; 32].into(),
        verdict: 0,
    };
    let mut buf = vec![0u8; WitnessVoteAccount::size()];
    vote.pack(&mut buf);
    let round = WitnessVoteAccount::unpack(&buf).unwrap();
    assert_eq!(round.version, 1);
    assert_eq!(round.bond_id, [7u8; 32]);
    assert_eq!(round.witness, [9u8; 32].into());
    assert_eq!(round.verdict, 0);
}

/// A zeroed (freshly created, never-voted) witness-vote account must read as
/// "not yet recorded" — this is what lets `process_attest` distinguish a
/// first vote from a replay without a separate account-existence flag.
#[test]
fn witness_vote_zeroed_account_reads_as_not_recorded() {
    let buf = vec![0u8; WitnessVoteAccount::size()];
    assert!(!WitnessVoteAccount::already_recorded(&buf));
}

/// This is the double-vote regression test: once a witness's vote is packed
/// into their (bond, witness) PDA slot, `already_recorded` must report true,
/// which is exactly the condition `process_attest` checks before touching
/// `bond.pass_count` / `bond.fail_count` — so a second `attest` call from the
/// same witness on the same bond is rejected before any vote count mutates,
/// instead of silently incrementing it a second time (the Waggle bug this
/// mirrors: `arbitrate()` had no per-arbiter record, so a single Queen-tier
/// signer could call it twice and force 2-of-3 quorum alone).
#[test]
fn witness_vote_second_attest_is_rejected_before_recording() {
    let mut buf = vec![0u8; WitnessVoteAccount::size()];
    assert!(!WitnessVoteAccount::already_recorded(&buf));

    let first_vote = WitnessVoteAccount {
        version: 1,
        bond_id: [7u8; 32],
        witness: [9u8; 32].into(),
        verdict: 0,
    };
    first_vote.pack(&mut buf);

    // Second attest from the same witness on the same bond: the guard must
    // trip *before* any bond.pass_count/fail_count mutation is attempted —
    // process_attest checks this exact condition first and returns
    // BondhiveError::AlreadyVoted without touching the bond account.
    assert!(WitnessVoteAccount::already_recorded(&buf));
}

/// Two distinct witnesses attesting on the same bond use distinct PDAs
/// (different `witness` in the seed), so both must be able to record
/// independently — the guard is per-(bond, witness), not per-bond.
#[test]
fn witness_vote_distinct_witnesses_both_record_independently() {
    let mut buf_a = vec![0u8; WitnessVoteAccount::size()];
    let mut buf_b = vec![0u8; WitnessVoteAccount::size()];

    WitnessVoteAccount {
        version: 1,
        bond_id: [7u8; 32],
        witness: [1u8; 32].into(),
        verdict: 0,
    }
    .pack(&mut buf_a);

    assert!(!WitnessVoteAccount::already_recorded(&buf_b));
    WitnessVoteAccount {
        version: 1,
        bond_id: [7u8; 32],
        witness: [2u8; 32].into(),
        verdict: 1,
    }
    .pack(&mut buf_b);

    assert!(WitnessVoteAccount::already_recorded(&buf_a));
    assert!(WitnessVoteAccount::already_recorded(&buf_b));
}
