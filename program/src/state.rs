//! On-chain account state for Bondhive.
//!
//! Manual byte (de)serialization to keep dependencies minimal. Every struct
//! has a version byte, a `pack`/`unpack` pair, and a fixed size so account
//! data can be validated on load.
//!
//! Custody note: token balances are *accounted* here (u64 fields). In the
//! production build these balances are wired to SPL Token accounts owned by
//! the PDAs; the accounting math is identical and is what the host tests
//! exercise. This crate deliberately avoids a token-program dependency so
//! `cargo check`/`cargo test` run without the Solana toolchain.

use solana_program::pubkey::Pubkey;

pub const VERSION: u8 = 1;

/// PoolAccount — global protocol state (PDA seed: b"pool").
pub struct PoolAccount {
    pub version: u8,
    pub authority: Pubkey,
    pub mint: Pubkey,
    pub treasury_bond: u64,      // BOND accumulated for buy-backs
    pub claims_pool: u64,        // BOND reserved for claim payouts + witness fees
    pub escrow: u64,             // BOND locked in active bonds
    pub staked_total: u64,       // BOND staked by pool backers
    pub burned_total: u64,       // cumulative burns
    pub premium_fee_bps: u16,    // 2000
    pub slash_claimant_bps: u16, // 7000
    pub slash_witness_bps: u16,  // 2000
    pub slash_burn_bps: u16,     // 1000
    pub milestone_volume: u64,   // cumulative bonded volume
    pub milestone_next: u64,     // next burn threshold
    pub bonds_active: u32,
    pub reserved: [u8; 32],
}

pub const POOL_SIZE: usize = 1 + 32 + 32 + 8 * 6 + 2 * 4 + 8 * 2 + 4 + 32;

impl PoolAccount {
    pub fn size() -> usize {
        POOL_SIZE
    }

    pub fn unpack(data: &[u8]) -> Result<Self, &'static str> {
        if data.len() < POOL_SIZE {
            return Err("pool account too small");
        }
        let mut p = 0usize;
        let version = data[p];
        p += 1;
        let authority = Pubkey::new_from_array(data[p..p + 32].try_into().unwrap());
        p += 32;
        let mint = Pubkey::new_from_array(data[p..p + 32].try_into().unwrap());
        p += 32;
        let mut rd = |p: &mut usize, n: usize| -> Vec<u8> {
            let v = data[*p..*p + n].to_vec();
            *p += n;
            v
        };
        let u64_at = |v: &[u8]| u64::from_le_bytes(v.try_into().unwrap());
        let u16_at = |v: &[u8]| u16::from_le_bytes(v.try_into().unwrap());
        let treasury_bond = u64_at(&rd(&mut p, 8));
        let claims_pool = u64_at(&rd(&mut p, 8));
        let escrow = u64_at(&rd(&mut p, 8));
        let staked_total = u64_at(&rd(&mut p, 8));
        let burned_total = u64_at(&rd(&mut p, 8));
        let premium_fee_bps = u16_at(&rd(&mut p, 2));
        let slash_claimant_bps = u16_at(&rd(&mut p, 2));
        let slash_witness_bps = u16_at(&rd(&mut p, 2));
        let slash_burn_bps = u16_at(&rd(&mut p, 2));
        let milestone_volume = u64_at(&rd(&mut p, 8));
        let milestone_next = u64_at(&rd(&mut p, 8));
        let bonds_active = u32::from_le_bytes(rd(&mut p, 4).try_into().unwrap());
        let reserved: [u8; 32] = rd(&mut p, 32).try_into().unwrap();
        Ok(PoolAccount {
            version,
            authority,
            mint,
            treasury_bond,
            claims_pool,
            escrow,
            staked_total,
            burned_total,
            premium_fee_bps,
            slash_claimant_bps,
            slash_witness_bps,
            slash_burn_bps,
            milestone_volume,
            milestone_next,
            bonds_active,
            reserved,
        })
    }

    pub fn pack(&self, data: &mut [u8]) {
        let mut p = 0usize;
        let mut w = |d: &mut [u8], p: &mut usize, bytes: &[u8]| {
            d[*p..*p + bytes.len()].copy_from_slice(bytes);
            *p += bytes.len();
        };
        w(data, &mut p, &[self.version]);
        w(data, &mut p, &self.authority.to_bytes());
        w(data, &mut p, &self.mint.to_bytes());
        w(data, &mut p, &self.treasury_bond.to_le_bytes());
        w(data, &mut p, &self.claims_pool.to_le_bytes());
        w(data, &mut p, &self.escrow.to_le_bytes());
        w(data, &mut p, &self.staked_total.to_le_bytes());
        w(data, &mut p, &self.burned_total.to_le_bytes());
        w(data, &mut p, &self.premium_fee_bps.to_le_bytes());
        w(data, &mut p, &self.slash_claimant_bps.to_le_bytes());
        w(data, &mut p, &self.slash_witness_bps.to_le_bytes());
        w(data, &mut p, &self.slash_burn_bps.to_le_bytes());
        w(data, &mut p, &self.milestone_volume.to_le_bytes());
        w(data, &mut p, &self.milestone_next.to_le_bytes());
        w(data, &mut p, &self.bonds_active.to_le_bytes());
        w(data, &mut p, &self.reserved);
    }
}

/// Bond lifecycle states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BondState {
    Offered = 0,
    Accepted = 1,
    ResolvedPass = 2,
    ResolvedFail = 3,
    Disputed = 4,
    Cancelled = 5,
}

/// BondAccount — one service bond (PDA seed: b"bond" || bond_id).
pub struct BondAccount {
    pub version: u8,
    pub state: u8,
    pub agent: Pubkey,
    pub counterparty: Pubkey,
    pub amount: u64,  // collateral locked
    pub premium: u64, // total premium (discounted)
    pub premium_paid: u64,
    pub witness_count: u8,
    pub pass_count: u8,
    pub fail_count: u8,
    pub created_at: i64,
    pub deadline: i64,
    pub resolved_at: i64,
    pub bond_id: [u8; 32],
    pub reserved: [u8; 16],
}

pub const BOND_SIZE: usize = 1 + 1 + 32 + 32 + 8 * 4 + 1 * 3 + 8 * 3 + 32 + 16;

impl BondAccount {
    pub fn size() -> usize {
        BOND_SIZE
    }

    pub fn unpack(data: &[u8]) -> Result<Self, &'static str> {
        if data.len() < BOND_SIZE {
            return Err("bond account too small");
        }
        let mut p = 0usize;
        let mut rd = |p: &mut usize, n: usize| -> Vec<u8> {
            let v = data[*p..*p + n].to_vec();
            *p += n;
            v
        };
        let u64_at = |v: &[u8]| u64::from_le_bytes(v.try_into().unwrap());
        let i64_at = |v: &[u8]| i64::from_le_bytes(v.try_into().unwrap());
        let version = data[p];
        p += 1;
        let state = data[p];
        p += 1;
        let agent = Pubkey::new_from_array(rd(&mut p, 32).try_into().unwrap());
        let counterparty = Pubkey::new_from_array(rd(&mut p, 32).try_into().unwrap());
        let amount = u64_at(&rd(&mut p, 8));
        let premium = u64_at(&rd(&mut p, 8));
        let premium_paid = u64_at(&rd(&mut p, 8));
        let witness_count = data[p];
        p += 1;
        let pass_count = data[p];
        p += 1;
        let fail_count = data[p];
        p += 1;
        let created_at = i64_at(&rd(&mut p, 8));
        let deadline = i64_at(&rd(&mut p, 8));
        let resolved_at = i64_at(&rd(&mut p, 8));
        let bond_id: [u8; 32] = rd(&mut p, 32).try_into().unwrap();
        let reserved: [u8; 16] = rd(&mut p, 16).try_into().unwrap();
        Ok(BondAccount {
            version,
            state,
            agent,
            counterparty,
            amount,
            premium,
            premium_paid,
            witness_count,
            pass_count,
            fail_count,
            created_at,
            deadline,
            resolved_at,
            bond_id,
            reserved,
        })
    }

    pub fn pack(&self, data: &mut [u8]) {
        let mut p = 0usize;
        let mut w = |d: &mut [u8], p: &mut usize, bytes: &[u8]| {
            d[*p..*p + bytes.len()].copy_from_slice(bytes);
            *p += bytes.len();
        };
        w(data, &mut p, &[self.version, self.state]);
        w(data, &mut p, &self.agent.to_bytes());
        w(data, &mut p, &self.counterparty.to_bytes());
        w(data, &mut p, &self.amount.to_le_bytes());
        w(data, &mut p, &self.premium.to_le_bytes());
        w(data, &mut p, &self.premium_paid.to_le_bytes());
        w(
            data,
            &mut p,
            &[self.witness_count, self.pass_count, self.fail_count],
        );
        w(data, &mut p, &self.created_at.to_le_bytes());
        w(data, &mut p, &self.deadline.to_le_bytes());
        w(data, &mut p, &self.resolved_at.to_le_bytes());
        w(data, &mut p, &self.bond_id);
        w(data, &mut p, &self.reserved);
    }
}

/// StakerAccount — one pool backer (PDA seed: b"staker" || owner).
pub struct StakerAccount {
    pub version: u8,
    pub owner: Pubkey,
    pub staked: u64,
    pub rewards_earned: u64,
    pub rewards_claimed: u64,
    pub joined_at: i64,
    pub reserved: [u8; 32],
}

pub const STAKER_SIZE: usize = 1 + 32 + 8 * 3 + 8 + 32;

impl StakerAccount {
    pub fn size() -> usize {
        STAKER_SIZE
    }

    pub fn unpack(data: &[u8]) -> Result<Self, &'static str> {
        if data.len() < STAKER_SIZE {
            return Err("staker account too small");
        }
        let mut p = 0usize;
        let mut rd = |p: &mut usize, n: usize| -> Vec<u8> {
            let v = data[*p..*p + n].to_vec();
            *p += n;
            v
        };
        let u64_at = |v: &[u8]| u64::from_le_bytes(v.try_into().unwrap());
        let i64_at = |v: &[u8]| i64::from_le_bytes(v.try_into().unwrap());
        let version = data[p];
        p += 1;
        let owner = Pubkey::new_from_array(rd(&mut p, 32).try_into().unwrap());
        let staked = u64_at(&rd(&mut p, 8));
        let rewards_earned = u64_at(&rd(&mut p, 8));
        let rewards_claimed = u64_at(&rd(&mut p, 8));
        let joined_at = i64_at(&rd(&mut p, 8));
        let reserved: [u8; 32] = rd(&mut p, 32).try_into().unwrap();
        Ok(StakerAccount {
            version,
            owner,
            staked,
            rewards_earned,
            rewards_claimed,
            joined_at,
            reserved,
        })
    }

    pub fn pack(&self, data: &mut [u8]) {
        let mut p = 0usize;
        let mut w = |d: &mut [u8], p: &mut usize, bytes: &[u8]| {
            d[*p..*p + bytes.len()].copy_from_slice(bytes);
            *p += bytes.len();
        };
        w(data, &mut p, &[self.version]);
        w(data, &mut p, &self.owner.to_bytes());
        w(data, &mut p, &self.staked.to_le_bytes());
        w(data, &mut p, &self.rewards_earned.to_le_bytes());
        w(data, &mut p, &self.rewards_claimed.to_le_bytes());
        w(data, &mut p, &self.joined_at.to_le_bytes());
        w(data, &mut p, &self.reserved);
    }
}

/// WitnessVoteAccount — one per (bond, witness) (PDA seed: b"witness_vote" ||
/// bond_id || witness). Its presence (`version == VERSION`) is the
/// double-vote guard for `process_attest`: the account is expected
/// pre-created (zeroed) by the same production wiring that creates
/// `bond`/`pool`/`staker` accounts; `unpack` on a zeroed buffer yields
/// `version == 0`, which `process_attest` reads as "not yet voted". Once a
/// witness attests, `version` is set to `VERSION` and the same witness
/// attesting again on the same bond is rejected before any vote count
/// mutates — the same PDA-uniqueness idea as an `init`-only account, applied
/// to this crate's manual account model instead of Anchor's.
pub struct WitnessVoteAccount {
    pub version: u8,
    pub bond_id: [u8; 32],
    pub witness: Pubkey,
    pub verdict: u8,
}

pub const WITNESS_VOTE_SIZE: usize = 1 + 32 + 32 + 1;

impl WitnessVoteAccount {
    pub fn size() -> usize {
        WITNESS_VOTE_SIZE
    }

    pub fn already_recorded(data: &[u8]) -> bool {
        !data.is_empty() && data[0] == VERSION
    }

    pub fn unpack(data: &[u8]) -> Result<Self, &'static str> {
        if data.len() < WITNESS_VOTE_SIZE {
            return Err("witness vote account too small");
        }
        let mut p = 0usize;
        let version = data[p];
        p += 1;
        let bond_id: [u8; 32] = data[p..p + 32].try_into().unwrap();
        p += 32;
        let witness = Pubkey::new_from_array(data[p..p + 32].try_into().unwrap());
        p += 32;
        let verdict = data[p];
        Ok(WitnessVoteAccount {
            version,
            bond_id,
            witness,
            verdict,
        })
    }

    pub fn pack(&self, data: &mut [u8]) {
        let mut p = 0usize;
        let mut w = |d: &mut [u8], p: &mut usize, bytes: &[u8]| {
            d[*p..*p + bytes.len()].copy_from_slice(bytes);
            *p += bytes.len();
        };
        w(data, &mut p, &[self.version]);
        w(data, &mut p, &self.bond_id);
        w(data, &mut p, &self.witness.to_bytes());
        w(data, &mut p, &[self.verdict]);
    }
}
