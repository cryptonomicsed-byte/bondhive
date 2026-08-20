//! Bondhive Solana program entrypoint.
//!
//! Implements the full bond lifecycle as on-chain instructions. Token custody
//! (SPL Token ATAs wired to the PDAs) is the production wiring; this crate
//! implements and verifies the *economic state machine* — escrow, premium
//! split, slashing, buy-back, burns, rewards — with all splits delegated to
//! the pure, host-tested `logic` module.
//!
//! Account conventions (all PDAs derived from program_id):
//!   pool    = PDA["pool"]
//!   bond    = PDA["bond" || bond_id]
//!   staker  = PDA["staker" || owner]

pub mod error;
pub mod instruction;
pub mod logic;
pub mod state;

use error::BondhiveError;
use instruction::BondhiveInstruction;
use solana_program::{
    account_info::AccountInfo, entrypoint, entrypoint::ProgramResult, msg,
    program_error::ProgramError, pubkey::Pubkey,
};
use state::{
    BondAccount, BondState, PoolAccount, StakerAccount, WitnessVoteAccount, STAKER_SIZE,
    WITNESS_VOTE_SIZE,
};

entrypoint!(process_instruction);

pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> ProgramResult {
    let ix = instruction::unpack(instruction_data).map_err(|_| {
        msg!("invalid instruction");
        BondhiveError::InvalidInstruction
    })?;
    msg!("Bondhive::process {:?}", std::mem::discriminant(&ix));

    match ix {
        BondhiveInstruction::Initialize { authority, mint } => {
            process_initialize(program_id, accounts, authority, mint)
        }
        BondhiveInstruction::Stake { amount } => process_stake(program_id, accounts, amount),
        BondhiveInstruction::Unstake { amount } => process_unstake(program_id, accounts, amount),
        BondhiveInstruction::CreateBond {
            bond_id,
            amount,
            premium,
            counterparty,
            deadline,
        } => process_create_bond(
            program_id,
            accounts,
            bond_id,
            amount,
            premium,
            counterparty,
            deadline,
        ),
        BondhiveInstruction::AcceptBond { bond_id } => {
            process_accept_bond(program_id, accounts, bond_id)
        }
        BondhiveInstruction::CancelBond { bond_id } => {
            process_cancel_bond(program_id, accounts, bond_id)
        }
        BondhiveInstruction::Attest { bond_id, verdict } => {
            process_attest(program_id, accounts, bond_id, verdict)
        }
        BondhiveInstruction::Resolve { bond_id, outcome } => {
            process_resolve(program_id, accounts, bond_id, outcome)
        }
        BondhiveInstruction::BuybackBurn { amount } => {
            process_buyback_burn(program_id, accounts, amount)
        }
        BondhiveInstruction::DistributeRewards {} => {
            process_distribute_rewards(program_id, accounts)
        }
        BondhiveInstruction::SetParams {
            premium_fee_bps,
            slash_claimant_bps,
            slash_witness_bps,
            slash_burn_bps,
        } => process_set_params(
            program_id,
            accounts,
            premium_fee_bps,
            slash_claimant_bps,
            slash_witness_bps,
            slash_burn_bps,
        ),
    }
}

// ---------------------------------------------------------------------------
// account helpers
// ---------------------------------------------------------------------------

fn pda(program_id: &Pubkey, seeds: &[&[u8]]) -> (Pubkey, u8) {
    Pubkey::find_program_address(seeds, program_id)
}

fn check_signer(account: &AccountInfo) -> Result<(), ProgramError> {
    if !account.is_signer {
        return Err(BondhiveError::SignerMismatch.into());
    }
    Ok(())
}

fn check_key(account: &AccountInfo, expected: &Pubkey) -> Result<(), ProgramError> {
    if account.key != expected {
        return Err(BondhiveError::InvalidAccount.into());
    }
    Ok(())
}

fn load_pool<'a>(
    program_id: &Pubkey,
    account: &'a AccountInfo,
) -> Result<PoolAccount, ProgramError> {
    let (key, _) = pda(program_id, &[b"pool"]);
    check_key(account, &key)?;
    PoolAccount::unpack(&account.data.borrow()).map_err(|_| BondhiveError::NotInitialized.into())
}

fn store_pool(account: &AccountInfo, pool: &PoolAccount) {
    let mut data = account.data.borrow_mut();
    pool.pack(&mut data);
}

// ---------------------------------------------------------------------------
// handlers
// ---------------------------------------------------------------------------

fn process_initialize(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    authority: Pubkey,
    mint: Pubkey,
) -> ProgramResult {
    let (signer, rest) = split2(accounts)?;
    let (pool, _) = split2(rest)?;
    check_signer(signer)?;
    check_key(signer, &authority)?;
    let (key, _) = pda(program_id, &[b"pool"]);
    check_key(pool, &key)?;
    let data = pool.data.borrow();
    if data.len() >= state::POOL_SIZE && data[0] == state::VERSION {
        return Err(BondhiveError::AlreadyInitialized.into());
    }
    drop(data);
    let mut p = PoolAccount {
        version: state::VERSION,
        authority,
        mint,
        treasury_bond: 0,
        claims_pool: 0,
        escrow: 0,
        staked_total: 0,
        burned_total: 0,
        premium_fee_bps: logic::PREMIUM_TREASURY_BPS as u16,
        slash_claimant_bps: logic::SLASH_CLAIMANT_BPS as u16,
        slash_witness_bps: logic::SLASH_WITNESS_BPS as u16,
        slash_burn_bps: logic::SLASH_BURN_BPS as u16,
        milestone_volume: 0,
        milestone_next: 1_000_000_000_000_000, // 1M BOND (9dp)
        bonds_active: 0,
        reserved: [0; 32],
    };
    // persist into a fresh buffer (re-init path)
    let mut buf = vec![0u8; state::POOL_SIZE];
    p.pack(&mut buf);
    let mut data = pool.data.borrow_mut();
    data.copy_from_slice(&buf);
    msg!("pool initialized: authority={authority}, mint={mint}");
    Ok(())
}

fn process_stake(program_id: &Pubkey, accounts: &[AccountInfo], amount: u64) -> ProgramResult {
    let (staker_signer, rest) = split2(accounts)?;
    let (staker_acct, rest) = split2(rest)?;
    let (pool_acct, _) = split2(rest)?;
    check_signer(staker_signer)?;
    let (skey, _) = pda(program_id, &[b"staker", staker_signer.key.as_ref()]);
    check_key(staker_acct, &skey)?;
    let mut pool = load_pool(program_id, pool_acct)?;
    pool.staked_total = pool
        .staked_total
        .checked_add(amount)
        .ok_or(BondhiveError::Overflow)?;
    store_pool(pool_acct, &pool);

    let mut data = staker_acct.data.borrow_mut();
    let mut staker = if data.len() >= STAKER_SIZE && data[0] == state::VERSION {
        StakerAccount::unpack(&data).map_err(|_| ProgramError::InvalidAccountData)?
    } else {
        StakerAccount {
            version: state::VERSION,
            owner: *staker_signer.key,
            staked: 0,
            rewards_earned: 0,
            rewards_claimed: 0,
            joined_at: 0,
            reserved: [0; 32],
        }
    };
    staker.staked = staker
        .staked
        .checked_add(amount)
        .ok_or(BondhiveError::Overflow)?;
    staker.pack(&mut data);
    msg!("staked {amount}");
    Ok(())
}

fn process_unstake(program_id: &Pubkey, accounts: &[AccountInfo], amount: u64) -> ProgramResult {
    let (staker_signer, rest) = split2(accounts)?;
    let (staker_acct, rest) = split2(rest)?;
    let (pool_acct, _) = split2(rest)?;
    check_signer(staker_signer)?;
    let (skey, _) = pda(program_id, &[b"staker", staker_signer.key.as_ref()]);
    check_key(staker_acct, &skey)?;
    let mut data = staker_acct.data.borrow_mut();
    let mut staker = StakerAccount::unpack(&data).map_err(|_| ProgramError::InvalidAccountData)?;
    if staker.staked < amount {
        return Err(BondhiveError::InsufficientStaked.into());
    }
    staker.staked -= amount;
    staker.pack(&mut data);
    drop(data);
    let mut pool = load_pool(program_id, pool_acct)?;
    pool.staked_total = pool
        .staked_total
        .checked_sub(amount)
        .ok_or(BondhiveError::InsufficientStaked)?;
    store_pool(pool_acct, &pool);
    msg!("unstaked {amount}");
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn process_create_bond(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    bond_id: [u8; 32],
    amount: u64,
    premium: u64,
    counterparty: Pubkey,
    deadline: i64,
) -> ProgramResult {
    let (agent, rest) = split2(accounts)?;
    let (bond_acct, _) = split2(rest)?;
    check_signer(agent)?;
    let (bkey, _) = pda(program_id, &[b"bond", &bond_id]);
    check_key(bond_acct, &bkey)?;
    let bond = BondAccount {
        version: state::VERSION,
        state: BondState::Offered as u8,
        agent: *agent.key,
        counterparty,
        amount,
        premium,
        premium_paid: 0,
        witness_count: 0,
        pass_count: 0,
        fail_count: 0,
        created_at: 0, // filled at accept with clock; simplified to 0 in accounting layer
        deadline,
        resolved_at: 0,
        bond_id,
        reserved: [0; 16],
    };
    let mut buf = vec![0u8; state::BOND_SIZE];
    bond.pack(&mut buf);
    bond_acct.data.borrow_mut().copy_from_slice(&buf);
    msg!("bond offered {amount} premium {premium}");
    Ok(())
}

fn process_accept_bond(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    bond_id: [u8; 32],
) -> ProgramResult {
    let (agent, rest) = split2(accounts)?;
    let (bond_acct, rest) = split2(rest)?;
    let (pool_acct, _) = split2(rest)?;
    check_signer(agent)?;
    let (bkey, _) = pda(program_id, &[b"bond", &bond_id]);
    check_key(bond_acct, &bkey)?;
    let mut data = bond_acct.data.borrow_mut();
    let mut bond = BondAccount::unpack(&data).map_err(|_| BondhiveError::BondNotFound)?;
    if bond.state != BondState::Offered as u8 {
        return Err(BondhiveError::WrongBondState.into());
    }
    if &bond.agent != agent.key {
        return Err(BondhiveError::SignerMismatch.into());
    }
    bond.state = BondState::Accepted as u8;
    bond.premium_paid = bond.premium;
    bond.pack(&mut data);
    drop(data);

    let mut pool = load_pool(program_id, pool_acct)?;
    let split = logic::premium_split(bond.premium);
    pool.escrow = pool
        .escrow
        .checked_add(bond.amount)
        .ok_or(BondhiveError::Overflow)?;
    pool.claims_pool = pool
        .claims_pool
        .checked_add(split.claims_share)
        .ok_or(BondhiveError::Overflow)?;
    pool.treasury_bond = pool
        .treasury_bond
        .checked_add(split.treasury_share)
        .ok_or(BondhiveError::Overflow)?;
    pool.milestone_volume = pool
        .milestone_volume
        .checked_add(bond.amount)
        .ok_or(BondhiveError::Overflow)?;
    pool.bonds_active = pool
        .bonds_active
        .checked_add(1)
        .ok_or(BondhiveError::Overflow)?;
    store_pool(pool_acct, &pool);
    msg!(
        "bond accepted; escrow += {}, claims += {}, treasury += {}",
        bond.amount,
        split.claims_share,
        split.treasury_share
    );
    Ok(())
}

fn process_cancel_bond(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    bond_id: [u8; 32],
) -> ProgramResult {
    let (agent, rest) = split2(accounts)?;
    let (bond_acct, _) = split2(rest)?;
    check_signer(agent)?;
    let (bkey, _) = pda(program_id, &[b"bond", &bond_id]);
    check_key(bond_acct, &bkey)?;
    let mut data = bond_acct.data.borrow_mut();
    let mut bond = BondAccount::unpack(&data).map_err(|_| BondhiveError::BondNotFound)?;
    if bond.state != BondState::Offered as u8 {
        return Err(BondhiveError::WrongBondState.into());
    }
    if &bond.agent != agent.key {
        return Err(BondhiveError::SignerMismatch.into());
    }
    bond.state = BondState::Cancelled as u8;
    bond.pack(&mut data);
    msg!("bond cancelled");
    Ok(())
}

fn process_attest(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    bond_id: [u8; 32],
    verdict: u8,
) -> ProgramResult {
    let (witness, rest) = split2(accounts)?;
    let (bond_acct, rest) = split2(rest)?;
    let (witness_vote_acct, _) = split2(rest)?;
    check_signer(witness)?;
    let (bkey, _) = pda(program_id, &[b"bond", &bond_id]);
    check_key(bond_acct, &bkey)?;
    let (vkey, _) = pda(program_id, &[b"witness_vote", &bond_id, witness.key.as_ref()]);
    check_key(witness_vote_acct, &vkey)?;

    // Double-vote guard: this witness's (bond, witness) PDA must not already
    // be recorded. Checked before any vote-count mutation on `bond`.
    let mut vote_data = witness_vote_acct.data.borrow_mut();
    if vote_data.len() < WITNESS_VOTE_SIZE {
        return Err(BondhiveError::InvalidAccount.into());
    }
    if WitnessVoteAccount::already_recorded(&vote_data) {
        return Err(BondhiveError::AlreadyVoted.into());
    }

    let mut data = bond_acct.data.borrow_mut();
    let mut bond = BondAccount::unpack(&data).map_err(|_| BondhiveError::BondNotFound)?;
    if bond.state != BondState::Accepted as u8 {
        return Err(BondhiveError::WrongBondState.into());
    }
    match verdict {
        0 => bond.pass_count = bond.pass_count.saturating_add(1),
        1 => bond.fail_count = bond.fail_count.saturating_add(1),
        _ => return Err(BondhiveError::InvalidVerdict.into()),
    }
    bond.witness_count = bond.witness_count.saturating_add(1);
    bond.pack(&mut data);

    let vote = WitnessVoteAccount {
        version: state::VERSION,
        bond_id,
        witness: *witness.key,
        verdict,
    };
    vote.pack(&mut vote_data);

    msg!("attestation recorded: verdict={verdict}");
    Ok(())
}

fn process_resolve(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    bond_id: [u8; 32],
    outcome: u8,
) -> ProgramResult {
    let (keeper, rest) = split2(accounts)?;
    let (bond_acct, rest) = split2(rest)?;
    let (pool_acct, _) = split2(rest)?;
    check_signer(keeper)?;
    // Resolve moves real escrowed funds (slash, burn) based solely on the
    // `outcome` the caller supplies — unlike Attest (which only records an
    // opinion), this instruction must be restricted to the pool authority
    // (the oracle/keeper role described in the README), matching the same
    // check already used for BuybackBurn and SetParams. Checked before any
    // account data is read/mutated so an unauthorized call fails clean.
    let mut pool = load_pool(program_id, pool_acct)?;
    if &pool.authority != keeper.key {
        return Err(BondhiveError::Unauthorized.into());
    }
    let (bkey, _) = pda(program_id, &[b"bond", &bond_id]);
    check_key(bond_acct, &bkey)?;
    let mut data = bond_acct.data.borrow_mut();
    let mut bond = BondAccount::unpack(&data).map_err(|_| BondhiveError::BondNotFound)?;
    if bond.state != BondState::Accepted as u8 {
        return Err(BondhiveError::WrongBondState.into());
    }
    pool.escrow = pool
        .escrow
        .checked_sub(bond.amount)
        .ok_or(BondhiveError::Overflow)?;
    pool.bonds_active = pool.bonds_active.saturating_sub(1);

    // Witness fee always paid from the claims pool (0.5% of bond value).
    let fee = logic::witness_fee(bond.amount);
    pool.claims_pool = pool.claims_pool.saturating_sub(fee);

    match outcome {
        0 => {
            bond.state = BondState::ResolvedPass as u8;
            // collateral returns to agent (accounted as escrow release);
            // rewards accrual for witnesses/holder yield handled by rewards engine
            msg!("resolved PASS; escrow released to agent");
        }
        1 => {
            bond.state = BondState::ResolvedFail as u8;
            let split = logic::slash_split(bond.amount);
            // claimant + witnesses paid from slashed escrow; burn is permanent
            pool.burned_total = pool
                .burned_total
                .checked_add(split.burn_share)
                .ok_or(BondhiveError::Overflow)?;
            msg!(
                "resolved FAIL; claimant {}, witnesses {}, burned {}",
                split.claimant_share,
                split.witness_share,
                split.burn_share
            );
        }
        _ => return Err(BondhiveError::InvalidVerdict.into()),
    }

    // Milestone burn check.
    if let Some(burn) = logic::milestone_burn(
        pool.milestone_volume,
        pool.milestone_next,
        pool.treasury_bond,
    ) {
        pool.treasury_bond = pool
            .treasury_bond
            .checked_sub(burn)
            .ok_or(BondhiveError::InsufficientTreasury)?;
        pool.burned_total = pool
            .burned_total
            .checked_add(burn)
            .ok_or(BondhiveError::Overflow)?;
        pool.milestone_next = pool.milestone_next.saturating_mul(10);
        msg!("milestone burn {burn}");
    }

    bond.resolved_at = 0; // clock plumbing simplified in accounting layer
    bond.pack(&mut data);
    store_pool(pool_acct, &pool);
    Ok(())
}

fn process_buyback_burn(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    amount: u64,
) -> ProgramResult {
    let (keeper, rest) = split2(accounts)?;
    let (pool_acct, _) = split2(rest)?;
    check_signer(keeper)?;
    let mut pool = load_pool(program_id, pool_acct)?;
    // Keeper must be the protocol authority (buy-backs are governed).
    if &pool.authority != keeper.key {
        return Err(BondhiveError::Unauthorized.into());
    }
    if pool.treasury_bond < amount {
        return Err(BondhiveError::InsufficientTreasury.into());
    }
    let split = logic::buyback_split(amount);
    pool.treasury_bond -= amount;
    pool.burned_total = pool
        .burned_total
        .checked_add(split.burn)
        .ok_or(BondhiveError::Overflow)?;
    // Redistributed half is credited to the pool's staked_total; individual
    // staker shares accrue via DistributeRewards.
    pool.staked_total = pool
        .staked_total
        .checked_add(split.redistribute)
        .ok_or(BondhiveError::Overflow)?;
    store_pool(pool_acct, &pool);
    msg!(
        "buyback {amount}: burn {}, redistribute {}",
        split.burn,
        split.redistribute
    );
    Ok(())
}

fn process_distribute_rewards(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
    let (staker_signer, rest) = split2(accounts)?;
    let (staker_acct, _pool_acct) = split2(rest)?;
    check_signer(staker_signer)?;
    let (skey, _) = pda(program_id, &[b"staker", staker_signer.key.as_ref()]);
    check_key(staker_acct, &skey)?;
    let mut data = staker_acct.data.borrow_mut();
    let mut staker = StakerAccount::unpack(&data).map_err(|_| ProgramError::InvalidAccountData)?;
    let pending = staker.rewards_earned - staker.rewards_claimed;
    staker.rewards_claimed += pending;
    staker.pack(&mut data);
    msg!("distributed {pending} rewards");
    Ok(())
}

fn process_set_params(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    premium_fee_bps: u16,
    slash_claimant_bps: u16,
    slash_witness_bps: u16,
    slash_burn_bps: u16,
) -> ProgramResult {
    let (authority, rest) = split2(accounts)?;
    let (pool_acct, _) = split2(rest)?;
    check_signer(authority)?;
    let mut pool = load_pool(program_id, pool_acct)?;
    if &pool.authority != authority.key {
        return Err(BondhiveError::Unauthorized.into());
    }
    let total: u64 = slash_claimant_bps as u64 + slash_witness_bps as u64 + slash_burn_bps as u64;
    if total != 10_000 {
        return Err(BondhiveError::InvalidVerdict.into()); // slash split must sum to 100%
    }
    pool.premium_fee_bps = premium_fee_bps;
    pool.slash_claimant_bps = slash_claimant_bps;
    pool.slash_witness_bps = slash_witness_bps;
    pool.slash_burn_bps = slash_burn_bps;
    store_pool(pool_acct, &pool);
    msg!("params updated");
    Ok(())
}

// ---------------------------------------------------------------------------
// small split helpers
// ---------------------------------------------------------------------------

fn split2<'a, 'info>(
    accounts: &'a [AccountInfo<'info>],
) -> Result<(&'a AccountInfo<'info>, &'a [AccountInfo<'info>]), ProgramError> {
    accounts
        .split_first()
        .ok_or(ProgramError::NotEnoughAccountKeys)
}
