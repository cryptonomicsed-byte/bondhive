//! BondScore — the reputation primitive of Bondhive.
//!
//! A per-pubkey score in [0, 100] derived from a chronological history of
//! verified bond outcomes:
//!
//! - start at 50 (neutral newborn)
//! - `+1` per verified success
//! - `-2` per verified failure
//! - `-5` per proven fraud (false attestation or violated bond)
//! - mean-reversion toward 50 with half-life 180 days, so stale history
//!   fades and reputation stays current
//!
//! The score drives the premium discount curve used across the protocol:
//!
//! | score | discount |
//! |-------|----------|
//! | < 60  | 0%       |
//! | 60-69 | 10%      |
//! | 70-79 | 25%      |
//! | >= 80 | 50%      |
//!
//! This module is deliberately free of Nostr and Solana dependencies so the
//! exact same math runs in the Rust core, the Solana program's host tests,
//! and (in principle) inside the program's on-chain validation.

/// The outcome of a resolved bond, as verified by witnesses/arbitration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Success,
    Failure,
    Fraud,
}

impl Outcome {
    pub fn delta(self) -> f64 {
        match self {
            Outcome::Success => 1.0,
            Outcome::Failure => -2.0,
            Outcome::Fraud => -5.0,
        }
    }
}

/// A single resolved outcome at a point in time (unix seconds).
#[derive(Debug, Clone, Copy)]
pub struct BondOutcome {
    pub outcome: Outcome,
    pub at: u64,
}

/// Half-life of reputation memory: deviation from neutral halves every 180 days.
pub const REPUTATION_HALF_LIFE_SECS: f64 = 180.0 * 86_400.0;
/// Neutral start / anchor score.
pub const NEUTRAL_SCORE: f64 = 50.0;

/// Compute the BondScore from a chronological outcome history, evaluated at `now`.
///
/// Events are applied in order; between events (and from the last event to
/// `now`) the score mean-reverts toward 50 exponentially. The result is
/// clamped to [0, 100].
pub fn compute_bond_score(history: &[BondOutcome], now: u64) -> f64 {
    let mut score = NEUTRAL_SCORE;
    let mut last_ts = 0u64;

    for entry in history {
        // Mean-reversion from the previous event time.
        if last_ts != 0 && entry.at > last_ts {
            score = revert(score, entry.at - last_ts);
        }
        score += entry.outcome.delta();
        last_ts = entry.at;
    }
    if last_ts != 0 && now > last_ts {
        score = revert(score, now - last_ts);
    }
    score.clamp(0.0, 100.0)
}

/// Exponential mean-reversion toward `NEUTRAL_SCORE`.
fn revert(score: f64, dt_secs: u64) -> f64 {
    let k = (dt_secs as f64 / REPUTATION_HALF_LIFE_SECS) * std::f64::consts::LN_2;
    NEUTRAL_SCORE + (score - NEUTRAL_SCORE) * (-k).exp()
}

/// Premium discount (0.0–0.5) for a given score, per the curve above.
pub fn premium_discount(score: f64) -> f64 {
    if score >= 80.0 {
        0.50
    } else if score >= 70.0 {
        0.25
    } else if score >= 60.0 {
        0.10
    } else {
        0.0
    }
}

/// Effective premium after discount: `base_premium * (1 - discount)`.
pub fn effective_premium(base_premium: u64, score: f64) -> u64 {
    let discounted = base_premium as f64 * (1.0 - premium_discount(score));
    discounted.round() as u64
}

/// Aggregate counts from a history — used for the `bondcount` / `bondsuccess`
/// / `bondfail` / `bondfraud` label values.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OutcomeCounts {
    pub success: u64,
    pub failure: u64,
    pub fraud: u64,
}

impl OutcomeCounts {
    pub fn from_history(history: &[BondOutcome]) -> Self {
        let mut c = OutcomeCounts::default();
        for e in history {
            match e.outcome {
                Outcome::Success => c.success += 1,
                Outcome::Failure => c.failure += 1,
                Outcome::Fraud => c.fraud += 1,
            }
        }
        c
    }

    pub fn total(&self) -> u64 {
        self.success + self.failure + self.fraud
    }
}

/// Convert an attestation `v` tag value ("pass"/"fail") plus arbitration
/// result into a canonical `Outcome`.
pub fn outcome_from_verdict(
    attestation_verdict: &str,
    arbitration: Option<&str>,
) -> Option<Outcome> {
    match arbitration {
        Some("uphold") => Some(Outcome::Failure),
        Some("reject") => Some(Outcome::Success),
        Some("split") => Some(Outcome::Failure), // split still counts as failure for reputation
        _ => match attestation_verdict {
            "pass" => Some(Outcome::Success),
            "fail" => Some(Outcome::Failure),
            _ => None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_800_000_000;

    fn hist(entries: &[(Outcome, u64)]) -> Vec<BondOutcome> {
        entries
            .iter()
            .map(|(o, t)| BondOutcome {
                outcome: *o,
                at: *t,
            })
            .collect()
    }

    #[test]
    fn newborn_is_neutral() {
        assert_eq!(compute_bond_score(&[], NOW), 50.0);
    }

    #[test]
    fn successes_raise_failures_lower() {
        // 3 successes + 1 failure = +1 net; mean-reversion is negligible over
        // a few hundred seconds (180-day half-life), so score must sit above 50.
        let h = hist(&[
            (Outcome::Success, NOW - 1000),
            (Outcome::Success, NOW - 700),
            (Outcome::Failure, NOW - 400),
            (Outcome::Success, NOW - 100),
        ]);
        let score = compute_bond_score(&h, NOW);
        assert!(score > 50.0, "expected > 50, got {score}");
        assert!(score < 100.0);
    }

    #[test]
    fn fraud_hurts_most() {
        let fraud = hist(&[(Outcome::Fraud, NOW - 100)]);
        let fail = hist(&[(Outcome::Failure, NOW - 100)]);
        assert!(compute_bond_score(&fraud, NOW) < compute_bond_score(&fail, NOW));
    }

    #[test]
    fn stale_history_reverts_to_neutral() {
        // Two successes five years ago should have nearly fully reverted.
        let h = hist(&[
            (Outcome::Success, NOW - 5 * 365 * 86_400),
            (Outcome::Success, NOW - 5 * 365 * 86_400 + 1),
        ]);
        let score = compute_bond_score(&h, NOW);
        assert!((score - 50.0).abs() < 1.0, "expected ~50, got {score}");
    }

    #[test]
    fn clamps_at_bounds() {
        let h = hist(&[(Outcome::Success, NOW - 1), (Outcome::Success, NOW - 2)]);
        let mut entries = h.clone();
        for i in 0..200 {
            entries.push(BondOutcome {
                outcome: Outcome::Success,
                at: NOW - 10_000 + i as u64,
            });
        }
        let score = compute_bond_score(&entries, NOW);
        assert!(score <= 100.0);
        let bad = hist(&[
            (Outcome::Fraud, NOW - 1),
            (Outcome::Fraud, NOW - 2),
            (Outcome::Fraud, NOW - 3),
        ]);
        assert!(compute_bond_score(&bad, NOW) >= 0.0);
    }

    #[test]
    fn discount_curve() {
        assert_eq!(premium_discount(50.0), 0.0);
        assert_eq!(premium_discount(60.0), 0.10);
        assert_eq!(premium_discount(75.0), 0.25);
        assert_eq!(premium_discount(80.0), 0.50);
        assert_eq!(premium_discount(100.0), 0.50);
    }

    #[test]
    fn effective_premium_example() {
        // A 3% premium on a 10,000 BOND bond = 300 BOND base.
        assert_eq!(effective_premium(300, 75.0), 225); // 25% off
        assert_eq!(effective_premium(300, 50.0), 300); // no discount
    }

    #[test]
    fn outcome_counts() {
        let h = hist(&[
            (Outcome::Success, 1),
            (Outcome::Success, 2),
            (Outcome::Failure, 3),
            (Outcome::Fraud, 4),
        ]);
        let c = OutcomeCounts::from_history(&h);
        assert_eq!(
            c,
            OutcomeCounts {
                success: 2,
                failure: 1,
                fraud: 1
            }
        );
        assert_eq!(c.total(), 4);
    }
}
