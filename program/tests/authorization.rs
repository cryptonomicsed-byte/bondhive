//! Host-side instruction-authorization tests.
//!
//! `tests/logic.rs` exercises the pure `logic`/`state` math only — it never
//! calls the actual instruction handlers in `lib.rs`, so the authorization
//! checks (`check_signer`, `check_key`, the pool-authority gate) had zero
//! coverage. `AccountInfo` can be constructed directly from owned buffers
//! without the full `solana-program-test` harness, so these tests do that
//! and call `process_instruction` for real — no Solana runtime/toolchain
//! required, consistent with the rest of this crate's host-testable design.

use bondhive_program::process_instruction;
use bondhive_program::state::{PoolAccount, VERSION};
use solana_program::account_info::AccountInfo;
use solana_program::pubkey::Pubkey;

fn pool_pda(program_id: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[b"pool"], program_id).0
}

fn bond_pda(program_id: &Pubkey, bond_id: &[u8; 32]) -> Pubkey {
    Pubkey::find_program_address(&[b"bond", bond_id], program_id).0
}

fn packed_pool(authority: Pubkey) -> Vec<u8> {
    let pool = PoolAccount {
        version: VERSION,
        authority,
        mint: Pubkey::new_unique(),
        treasury_bond: 0,
        claims_pool: 0,
        escrow: 5_000, // pre-funded so Resolve's checked_sub has something to release
        staked_total: 0,
        burned_total: 0,
        premium_fee_bps: 2000,
        slash_claimant_bps: 7000,
        slash_witness_bps: 2000,
        slash_burn_bps: 1000,
        milestone_volume: 0,
        milestone_next: 1_000_000_000_000_000,
        bonds_active: 1,
        reserved: [0; 32],
    };
    let mut buf = vec![0u8; PoolAccount::size()];
    pool.pack(&mut buf);
    buf
}

fn packed_accepted_bond(agent: Pubkey, bond_id: [u8; 32]) -> Vec<u8> {
    use bondhive_program::state::{BondAccount, BondState};
    let bond = BondAccount {
        version: VERSION,
        state: BondState::Accepted as u8,
        agent,
        counterparty: Pubkey::new_unique(),
        amount: 5_000,
        premium: 300,
        premium_paid: 300,
        witness_count: 0,
        pass_count: 0,
        fail_count: 0,
        created_at: 0,
        deadline: 0,
        resolved_at: 0,
        bond_id,
        reserved: [0; 16],
    };
    let mut buf = vec![0u8; BondAccount::size()];
    bond.pack(&mut buf);
    buf
}

/// Build a resolve instruction call and return its ProgramResult, using a
/// caller-supplied `keeper` pubkey that may or may not match the pool's
/// stored authority.
fn try_resolve(keeper_is_authority: bool) -> Result<(), solana_program::program_error::ProgramError> {
    let program_id = Pubkey::new_unique();
    let authority = Pubkey::new_unique();
    let agent = Pubkey::new_unique();
    let bond_id = [7u8; 32];

    let keeper_key = if keeper_is_authority {
        authority
    } else {
        Pubkey::new_unique() // some other, unrelated signer
    };

    let mut keeper_lamports = 0u64;
    let mut bond_data = packed_accepted_bond(agent, bond_id);
    let mut bond_lamports = 0u64;
    let mut pool_data = packed_pool(authority);
    let mut pool_lamports = 0u64;

    let bond_key = bond_pda(&program_id, &bond_id);
    let pool_key = pool_pda(&program_id);
    let owner = program_id;

    let keeper_info = AccountInfo::new(
        &keeper_key,
        true, // is_signer
        false,
        &mut keeper_lamports,
        &mut [],
        &owner,
        false,
        0,
    );
    let bond_info = AccountInfo::new(
        &bond_key,
        false,
        true,
        &mut bond_lamports,
        &mut bond_data,
        &owner,
        false,
        0,
    );
    let pool_info = AccountInfo::new(
        &pool_key,
        false,
        true,
        &mut pool_lamports,
        &mut pool_data,
        &owner,
        false,
        0,
    );

    let accounts = [keeper_info, bond_info, pool_info];
    // Resolve wire format (instruction.rs::unpack, opcode 7):
    // [7] || bond_id (32 bytes) || outcome (1 byte)
    let mut data = vec![7u8];
    data.extend_from_slice(&bond_id);
    data.push(0); // outcome = PASS
    process_instruction(&program_id, &accounts, &data)
}

#[test]
fn resolve_rejects_non_authority_keeper() {
    let result = try_resolve(false);
    assert!(
        result.is_err(),
        "an arbitrary signer must not be able to resolve a bond — only the pool authority should"
    );
}

#[test]
fn resolve_accepts_authority_keeper() {
    let result = try_resolve(true);
    assert!(
        result.is_ok(),
        "the real pool authority must still be able to resolve bonds: {result:?}"
    );
}
