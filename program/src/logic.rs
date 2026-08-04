//! Bondhive on-chain accounting engine.
//!
//! Every economic rule of the protocol lives here as pure functions with no
//! Solana dependencies, so the exact same numbers are unit-tested on the host
//! and executed by the program. Production custody of the BOND token sits with
//! the SPL Token program (ATAs are wired to the PDAs); this module decides how
//! those balances move. See docs/tokenomics.md for the canonical spec.

/// Premium split: 80% to the claims pool, 20% to the protocol treasury
/// (buy-back engine). Denominators are basis points.
pub const PREMIUM_CLAIMS_BPS: u64 = 8_000;
pub const PREMIUM_TREASURY_BPS: u64 = 2_000;

/// Slash split on a failed bond: 70% to the counterparty who lost because the
/// agent failed, 20% to honest witnesses (pro-rata), 10% burned.
pub const SLASH_CLAIMANT_BPS: u64 = 7_000;
pub const SLASH_WITNESS_BPS: u64 = 2_000;
pub const SLASH_BURN_BPS: u64 = 1_000;

/// Per-attestation witness fee, paid from the claims pool: 0.5% of bond value.
pub const WITNESS_FEE_BPS: u64 = 50;

/// Buy-back split: 50% of purchased BOND is burned, 50% redistributed to
/// stakers pro-rata.
pub const BUYBACK_BURN_BPS: u64 = 5_000;

/// Milestone burn: when cumulative bond volume crosses a threshold, burn
/// 0.5% of the treasury's BOND balance (governance-gated).
pub const MILESTONE_BURN_BPS: u64 = 50;

/// BondScore premium discount curve (mirrors core::score::premium_discount).
pub fn premium_discount(score: u16) -> u16 {
    if score >= 80 {
        50
    } else if score >= 70 {
        25
    } else if score >= 60 {
        10
    } else {
        0
    }
}

/// Effective premium after discount: `base * (1000 - discount_bps*10) / 1000`.
pub fn effective_premium(base_premium: u64, score: u16) -> u64 {
    let discount = premium_discount(score) as u64;
    base_premium * (1000 - discount * 10) / 1000
}

/// Split a premium into claims-pool and treasury shares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PremiumSplit {
    pub claims_share: u64,
    pub treasury_share: u64,
}

pub fn premium_split(premium: u64) -> PremiumSplit {
    PremiumSplit {
        claims_share: premium * PREMIUM_CLAIMS_BPS / 10_000,
        treasury_share: premium * PREMIUM_TREASURY_BPS / 10_000,
    }
}

/// Split slashed collateral between claimant, witnesses and burn.
///
/// Claimant and witness shares are floored; the remainder is assigned to the
/// burn so the split always sums exactly to the collateral (no dust loss).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlashSplit {
    pub claimant_share: u64,
    pub witness_share: u64,
    pub burn_share: u64,
}

pub fn slash_split(collateral: u64) -> SlashSplit {
    let claimant_share = collateral * SLASH_CLAIMANT_BPS / 10_000;
    let witness_share = collateral * SLASH_WITNESS_BPS / 10_000;
    let burn_share = collateral - claimant_share - witness_share; // remainder to burn
    SlashSplit {
        claimant_share,
        witness_share,
        burn_share,
    }
}

/// Witness fee for a single attestation.
pub fn witness_fee(bond_amount: u64) -> u64 {
    bond_amount * WITNESS_FEE_BPS / 10_000
}

/// Split a buy-back purchase into burn + staker redistribution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuybackSplit {
    pub burn: u64,
    pub redistribute: u64,
}

pub fn buyback_split(amount: u64) -> BuybackSplit {
    BuybackSplit {
        burn: amount * BUYBACK_BURN_BPS / 10_000,
        redistribute: amount - (amount * BUYBACK_BURN_BPS / 10_000),
    }
}

/// Milestone burn result: `Some(amount)` when `volume` crossed the threshold.
pub fn milestone_burn(volume: u64, threshold: u64, treasury_balance: u64) -> Option<u64> {
    if volume >= threshold && treasury_balance > 0 {
        let burn = treasury_balance * MILESTONE_BURN_BPS / 10_000;
        Some(burn.max(1))
    } else {
        None
    }
}

/// Quorum rule: a bond resolves when at least `quorum` witnesses have attested
/// and `passes` of them voted pass.
pub fn resolves_pass(attestations: u8, passes: u8, quorum: u8) -> bool {
    attestations >= quorum && passes * 2 > attestations
}

/// Max bond size a given BondScore tier may post (governance params).
pub fn max_bond_for_score(score: u16) -> u64 {
    if score >= 80 {
        1_000_000_000_000_000 // 1,000,000 BOND (9dp)
    } else if score >= 60 {
        100_000_000_000_000 // 100,000 BOND
    } else if score >= 40 {
        10_000_000_000_000 // 10,000 BOND
    } else {
        1_000_000_000_000 // 1,000 BOND
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn premium_split_is_80_20() {
        let s = premium_split(300);
        assert_eq!(s.claims_share, 240);
        assert_eq!(s.treasury_share, 60);
    }

    #[test]
    fn slash_split_is_70_20_10() {
        let s = slash_split(10_000);
        assert_eq!(s.claimant_share, 7_000);
        assert_eq!(s.witness_share, 2_000);
        assert_eq!(s.burn_share, 1_000);
    }

    #[test]
    fn slash_split_sums_to_collateral_exactly() {
        for collateral in [1, 2, 333, 10_000, 999_999_999_999] {
            let s = slash_split(collateral);
            assert_eq!(
                s.claimant_share + s.witness_share + s.burn_share,
                collateral
            );
        }
    }

    #[test]
    fn witness_fee_is_50bps() {
        assert_eq!(witness_fee(10_000), 50);
        assert_eq!(witness_fee(1_000_000), 5_000);
    }

    #[test]
    fn buyback_split_halves() {
        let s = buyback_split(1_000);
        assert_eq!(s.burn, 500);
        assert_eq!(s.redistribute, 500);
    }

    #[test]
    fn milestone_burn_only_crosses_threshold() {
        assert_eq!(milestone_burn(999_999, 1_000_000, 100_000), None);
        let burn = milestone_burn(1_000_000, 1_000_000, 100_000);
        assert_eq!(burn, Some(500)); // 0.5% of 100k
        assert_eq!(milestone_burn(1_000_000, 1_000_000, 0), None);
    }

    #[test]
    fn quorum_majority() {
        assert!(!resolves_pass(2, 1, 3)); // not enough attestations
        assert!(resolves_pass(3, 2, 3)); // 2/3 pass
        assert!(!resolves_pass(3, 1, 3)); // 1/3 pass -> fail
        assert!(resolves_pass(3, 3, 3)); // unanimous
    }

    #[test]
    fn effective_premium_matches_core_curve() {
        assert_eq!(effective_premium(300, 50), 300);
        assert_eq!(effective_premium(300, 60), 270);
        assert_eq!(effective_premium(300, 75), 225);
        assert_eq!(effective_premium(300, 90), 150);
    }

    #[test]
    fn max_bond_tiers() {
        assert_eq!(max_bond_for_score(30), 1_000_000_000_000);
        assert_eq!(max_bond_for_score(90), 1_000_000_000_000_000);
    }
}
