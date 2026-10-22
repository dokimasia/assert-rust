// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The coverage test: whether a label covers a required share of the cases.
//!
//! A requirement is decided with the Wilson score interval of the label's
//! share, with the constants that `QuickCheck`'s `checkCoverage` uses: a
//! certainty of 10^9 and a tolerance of 0.9. The runner checks every
//! requirement after `CHECKS[0]` times `cases` valid cases, and again each
//! time that count doubles, up to the last check. The test uses arithmetic
//! and a square root alone, which IEEE 754 rounds exactly, so every language
//! decides the same way from the same counts.

/// The standard normal quantile at 1 - 5e-10: `QuickCheck`'s certainty of
/// 10^9, split between the two tails.
const Z: f64 = 6.109_410_191_663_286;

/// A requirement is met when the lower bound reaches this fraction of the
/// required share.
const TOLERANCE: f64 = 0.9;

/// The checks, as multiples of `cases` valid cases. The last is the last
/// check of the run.
pub(crate) const CHECKS: [usize; 4] = [1, 2, 4, 8];

/// What a check decides about one requirement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// The label covers the share.
    Met,
    /// The label cannot cover the share.
    Refuted,
    /// The counts decide neither yet.
    Undecided,
    /// The last check, or an exact count, found the share uncovered.
    Unmet,
}

impl Verdict {
    /// Returns the name of the verdict, as a record states it.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Met => "met",
            Self::Refuted => "refuted",
            Self::Undecided => "undecided",
            Self::Unmet => "unmet",
        }
    }
}

/// Returns a count as a float, which states every count below 2^53 exactly.
#[expect(
    clippy::cast_precision_loss,
    reason = "a count of cases is far below 2^53, which a float states exactly"
)]
fn float(count: usize) -> f64 {
    count as f64
}

/// Returns the Wilson score bound of `k` successes in `n` trials at quantile
/// `z`: the upper bound for a positive `z`, and the lower one for a negative
/// one.
fn bound(k: usize, n: usize, z: f64) -> f64 {
    let trials = float(n);
    let share = float(k) / trials;
    let weight = z * z / trials;
    let centre = share + weight / 2.0;
    let spread = z * (share * (1.0 - share) / trials + weight / (4.0 * trials)).sqrt();
    (centre + spread) / (1.0 + weight)
}

/// Returns the verdict on one requirement at one check.
///
/// `exact` marks a run that tested every input of its domain, whose shares
/// are exact. An exact share, and an undecided share at the last check, are
/// met when `k / n` is at least the tolerance times the share, and unmet
/// otherwise. A run without a valid case has no share, and leaves the
/// requirement unmet.
#[must_use]
pub fn verdict(k: usize, n: usize, share: f64, last: bool, exact: bool) -> Verdict {
    if n == 0 {
        return if last || exact {
            Verdict::Unmet
        } else {
            Verdict::Undecided
        };
    }
    if !exact {
        if bound(k, n, -Z) >= TOLERANCE * share {
            return Verdict::Met;
        }
        if bound(k, n, Z) < share {
            return Verdict::Refuted;
        }
        if !last {
            return Verdict::Undecided;
        }
    }
    if float(k) / float(n) >= TOLERANCE * share {
        Verdict::Met
    } else {
        Verdict::Unmet
    }
}
