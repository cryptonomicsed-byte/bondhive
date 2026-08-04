/**
 * Bondhive economics — TypeScript mirror of program/src/logic.rs.
 *
 * PARITY CONTRACT: every constant and function here must produce identical
 * numbers to the Rust `logic` module. The host tests on both sides assert
 * the same values (see program/tests/logic.rs and src/selftest.ts).
 */

export const PREMIUM_CLAIMS_BPS = 8_000; // 80%
export const PREMIUM_TREASURY_BPS = 2_000; // 20%
export const SLASH_CLAIMANT_BPS = 7_000; // 70%
export const SLASH_WITNESS_BPS = 2_000; // 20%
export const SLASH_BURN_BPS = 1_000; // 10%
export const WITNESS_FEE_BPS = 50; // 0.5%
export const BUYBACK_BURN_BPS = 5_000; // 50%
export const MILESTONE_BURN_BPS = 50; // 0.5%

export interface PremiumSplit {
  claimsShare: number;
  treasuryShare: number;
}

export interface SlashSplit {
  claimantShare: number;
  witnessShare: number;
  burnShare: number;
}

export interface BuybackSplit {
  burn: number;
  redistribute: number;
}

export function premiumDiscount(score: number): number {
  if (score >= 80) return 50;
  if (score >= 70) return 25;
  if (score >= 60) return 10;
  return 0;
}

export function effectivePremium(basePremium: number, score: number): number {
  const discount = premiumDiscount(score);
  return Math.round((basePremium * (1000 - discount * 10)) / 1000);
}

export function premiumSplit(premium: number): PremiumSplit {
  return {
    claimsShare: Math.floor((premium * PREMIUM_CLAIMS_BPS) / 10_000),
    treasuryShare: Math.floor((premium * PREMIUM_TREASURY_BPS) / 10_000),
  };
}

export function slashSplit(collateral: number): SlashSplit {
  const claimantShare = Math.floor((collateral * SLASH_CLAIMANT_BPS) / 10_000);
  const witnessShare = Math.floor((collateral * SLASH_WITNESS_BPS) / 10_000);
  const burnShare = collateral - claimantShare - witnessShare; // remainder to burn
  return { claimantShare, witnessShare, burnShare };
}

export function witnessFee(bondAmount: number): number {
  return Math.floor((bondAmount * WITNESS_FEE_BPS) / 10_000);
}

export function buybackSplit(amount: number): BuybackSplit {
  const burn = Math.floor((amount * BUYBACK_BURN_BPS) / 10_000);
  return { burn, redistribute: amount - burn };
}

export function milestoneBurn(volume: number, threshold: number, treasuryBalance: number): number | null {
  if (volume >= threshold && treasuryBalance > 0) {
    return Math.max(Math.floor((treasuryBalance * MILESTONE_BURN_BPS) / 10_000), 1);
  }
  return null;
}

export function resolvesPass(attestations: number, passes: number, quorum: number): boolean {
  return attestations >= quorum && passes * 2 > attestations;
}
