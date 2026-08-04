//! Instruction definitions and parsing.
//!
//! Layout: `[discriminant: u8][fixed fields]`. All integers little-endian.

use solana_program::pubkey::Pubkey;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BondhiveInstruction {
    /// Initialize the protocol pool. Signer: authority. Creates PoolAccount.
    Initialize { authority: Pubkey, mint: Pubkey },

    /// Stake BOND into the mutual pool. Signer: staker.
    Stake { amount: u64 },

    /// Unstake BOND (only previously staked balance). Signer: staker.
    Unstake { amount: u64 },

    /// Offer a service bond. Signer: the agent. Creates BondAccount.
    CreateBond {
        bond_id: [u8; 32],
        amount: u64,
        premium: u64,
        counterparty: Pubkey,
        deadline: i64,
    },

    /// Accept the bond, lock collateral, split premium. Signer: agent.
    AcceptBond { bond_id: [u8; 32] },

    /// Cancel an offered (not yet accepted) bond. Signer: agent.
    CancelBond { bond_id: [u8; 32] },

    /// Record a witness attestation (verdict: 0 = pass, 1 = fail).
    Attest { bond_id: [u8; 32], verdict: u8 },

    /// Resolve a bond after quorum: 0 = pass, 1 = fail.
    Resolve { bond_id: [u8; 32], outcome: u8 },

    /// Keeper-driven buy-back + burn from treasury.
    BuybackBurn { amount: u64 },

    /// Distribute accumulated rewards to a staker.
    DistributeRewards {},

    /// Governance: update fee/slash parameters. Signer: authority.
    SetParams {
        premium_fee_bps: u16,
        slash_claimant_bps: u16,
        slash_witness_bps: u16,
        slash_burn_bps: u16,
    },
}

pub fn unpack(input: &[u8]) -> Result<BondhiveInstruction, &'static str> {
    let (&disc, rest) = input.split_first().ok_or("empty instruction")?;
    match disc {
        0 => {
            // authority(32) + mint(32)
            let authority = Pubkey::new_from_array(rest[0..32].try_into().unwrap());
            let mint = Pubkey::new_from_array(rest[32..64].try_into().unwrap());
            Ok(BondhiveInstruction::Initialize { authority, mint })
        }
        1 => Ok(BondhiveInstruction::Stake {
            amount: u64_at(rest, 0)?,
        }),
        2 => Ok(BondhiveInstruction::Unstake {
            amount: u64_at(rest, 0)?,
        }),
        3 => {
            let bond_id: [u8; 32] = rest[0..32].try_into().unwrap();
            let amount = u64_at(rest, 32)?;
            let premium = u64_at(rest, 40)?;
            let counterparty = Pubkey::new_from_array(rest[48..80].try_into().unwrap());
            let deadline = i64::from_le_bytes(rest[80..88].try_into().unwrap());
            Ok(BondhiveInstruction::CreateBond {
                bond_id,
                amount,
                premium,
                counterparty,
                deadline,
            })
        }
        4 => {
            let bond_id: [u8; 32] = rest[0..32].try_into().unwrap();
            Ok(BondhiveInstruction::AcceptBond { bond_id })
        }
        5 => {
            let bond_id: [u8; 32] = rest[0..32].try_into().unwrap();
            Ok(BondhiveInstruction::CancelBond { bond_id })
        }
        6 => {
            let bond_id: [u8; 32] = rest[0..32].try_into().unwrap();
            let verdict = *rest.get(32).ok_or("missing verdict")?;
            Ok(BondhiveInstruction::Attest { bond_id, verdict })
        }
        7 => {
            let bond_id: [u8; 32] = rest[0..32].try_into().unwrap();
            let outcome = *rest.get(32).ok_or("missing outcome")?;
            Ok(BondhiveInstruction::Resolve { bond_id, outcome })
        }
        8 => Ok(BondhiveInstruction::BuybackBurn {
            amount: u64_at(rest, 0)?,
        }),
        9 => Ok(BondhiveInstruction::DistributeRewards {}),
        10 => {
            let premium_fee_bps = u16_at(rest, 0)?;
            let slash_claimant_bps = u16_at(rest, 2)?;
            let slash_witness_bps = u16_at(rest, 4)?;
            let slash_burn_bps = u16_at(rest, 6)?;
            Ok(BondhiveInstruction::SetParams {
                premium_fee_bps,
                slash_claimant_bps,
                slash_witness_bps,
                slash_burn_bps,
            })
        }
        _ => Err("unknown instruction discriminant"),
    }
}

fn u64_at(data: &[u8], off: usize) -> Result<u64, &'static str> {
    data.get(off..off + 8)
        .map(|s| u64::from_le_bytes(s.try_into().unwrap()))
        .ok_or("truncated u64")
}

fn u16_at(data: &[u8], off: usize) -> Result<u16, &'static str> {
    data.get(off..off + 2)
        .map(|s| u16::from_le_bytes(s.try_into().unwrap()))
        .ok_or("truncated u16")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pk(b: u8) -> Pubkey {
        Pubkey::new_from_array([b; 32])
    }

    #[test]
    fn roundtrip_all_instructions() {
        let cases = vec![
            BondhiveInstruction::Initialize {
                authority: pk(1),
                mint: pk(2),
            },
            BondhiveInstruction::Stake { amount: 123 },
            BondhiveInstruction::Unstake { amount: 1 },
            BondhiveInstruction::CreateBond {
                bond_id: [7; 32],
                amount: 10_000,
                premium: 300,
                counterparty: pk(9),
                deadline: 1_800_000_000,
            },
            BondhiveInstruction::Attest {
                bond_id: [7; 32],
                verdict: 0,
            },
            BondhiveInstruction::Resolve {
                bond_id: [7; 32],
                outcome: 1,
            },
            BondhiveInstruction::BuybackBurn { amount: 5_000 },
            BondhiveInstruction::DistributeRewards {},
            BondhiveInstruction::SetParams {
                premium_fee_bps: 2000,
                slash_claimant_bps: 7000,
                slash_witness_bps: 2000,
                slash_burn_bps: 1000,
            },
        ];
        for case in cases {
            let bytes = pack(&case);
            assert_eq!(unpack(&bytes).unwrap(), case);
        }
    }

    fn pack(ix: &BondhiveInstruction) -> Vec<u8> {
        let mut out = vec![];
        match ix {
            BondhiveInstruction::Initialize { authority, mint } => {
                out.push(0);
                out.extend_from_slice(&authority.to_bytes());
                out.extend_from_slice(&mint.to_bytes());
            }
            BondhiveInstruction::Stake { amount } => {
                out.push(1);
                out.extend_from_slice(&amount.to_le_bytes());
            }
            BondhiveInstruction::Unstake { amount } => {
                out.push(2);
                out.extend_from_slice(&amount.to_le_bytes());
            }
            BondhiveInstruction::CreateBond {
                bond_id,
                amount,
                premium,
                counterparty,
                deadline,
            } => {
                out.push(3);
                out.extend_from_slice(bond_id);
                out.extend_from_slice(&amount.to_le_bytes());
                out.extend_from_slice(&premium.to_le_bytes());
                out.extend_from_slice(&counterparty.to_bytes());
                out.extend_from_slice(&deadline.to_le_bytes());
            }
            BondhiveInstruction::AcceptBond { bond_id } => {
                out.push(4);
                out.extend_from_slice(bond_id);
            }
            BondhiveInstruction::CancelBond { bond_id } => {
                out.push(5);
                out.extend_from_slice(bond_id);
            }
            BondhiveInstruction::Attest { bond_id, verdict } => {
                out.push(6);
                out.extend_from_slice(bond_id);
                out.push(*verdict);
            }
            BondhiveInstruction::Resolve { bond_id, outcome } => {
                out.push(7);
                out.extend_from_slice(bond_id);
                out.push(*outcome);
            }
            BondhiveInstruction::BuybackBurn { amount } => {
                out.push(8);
                out.extend_from_slice(&amount.to_le_bytes());
            }
            BondhiveInstruction::DistributeRewards {} => {
                out.push(9);
            }
            BondhiveInstruction::SetParams {
                premium_fee_bps,
                slash_claimant_bps,
                slash_witness_bps,
                slash_burn_bps,
            } => {
                out.push(10);
                out.extend_from_slice(&premium_fee_bps.to_le_bytes());
                out.extend_from_slice(&slash_claimant_bps.to_le_bytes());
                out.extend_from_slice(&slash_witness_bps.to_le_bytes());
                out.extend_from_slice(&slash_burn_bps.to_le_bytes());
            }
        }
        out
    }
}
