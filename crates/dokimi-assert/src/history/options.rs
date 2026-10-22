// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The settings of a check of linearizability: a builder whose methods each
//! state one setting.

use std::time::Duration;

/// The steps that the search of one partition may spend by default.
const BUDGET: u64 = 10_000_000;

/// The bits that the memo of one partition may count by default: 2^33, which
/// is 1 GiB.
const MEMO_LIMIT: u64 = 1 << 33;

/// The settings of a check of linearizability.
///
/// Each method states one setting and returns the settings, so a caller
/// chains them. A free function of the same name, such as [`budget`], starts
/// the chain, and `Default::default()` states no setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub struct Options {
    pub(crate) budget: u64,
    pub(crate) memo_limit: u64,
    pub(crate) time_limit: Duration,
    pub(crate) workers: usize,
}

/// States 10,000,000 steps, a memo of 2^33 bits, no time limit and one
/// worker.
impl Default for Options {
    fn default() -> Self {
        Self {
            budget: BUDGET,
            memo_limit: MEMO_LIMIT,
            time_limit: Duration::ZERO,
            workers: 1,
        }
    }
}

impl Options {
    /// States the steps that the search of one partition may spend. A step
    /// is one call of the spec's `next`. The search takes no step that would
    /// pass the budget, and ends as undecided at the limit `steps`.
    ///
    /// # Panics
    ///
    /// Panics for 0 steps.
    pub fn budget(mut self, steps: u64) -> Self {
        assert!(steps >= 1, "history: budget(0) admits no step");
        self.budget = steps;
        self
    }

    /// States the bits that the memo of one partition's search may count.
    /// Each configuration in the memo counts one bit for each call of the
    /// partition. The search stores no configuration that would pass the
    /// limit, and ends as undecided at the limit `memo`.
    ///
    /// # Panics
    ///
    /// Panics for 0 bits.
    pub fn memo_limit(mut self, bits: u64) -> Self {
        assert!(bits >= 1, "history: memo_limit(0) admits no configuration");
        self.memo_limit = bits;
        self
    }

    /// States the time that the whole check may take on the platform clock,
    /// and zero for no limit. The search reads the clock before its first
    /// step and every 1,024 steps after it, and once the time has passed it
    /// ends as undecided at the limit `time`. A time limit makes the outcome
    /// depend on the machine.
    pub fn time_limit(mut self, limit: Duration) -> Self {
        self.time_limit = limit;
        self
    }

    /// States the partitions that the check searches at once, on threads.
    /// The check reports what a check on one worker reports.
    ///
    /// # Panics
    ///
    /// Panics for 0 workers.
    pub fn workers(mut self, n: usize) -> Self {
        assert!(n >= 1, "history: workers(0) searches nothing");
        self.workers = n;
        self
    }
}

/// Returns the settings with the steps that one partition's search may
/// spend.
///
/// # Panics
///
/// Panics for 0 steps.
pub fn budget(steps: u64) -> Options {
    Options::default().budget(steps)
}

/// Returns the settings with the bits that one partition's memo may count.
///
/// # Panics
///
/// Panics for 0 bits.
pub fn memo_limit(bits: u64) -> Options {
    Options::default().memo_limit(bits)
}

/// Returns the settings with the time that the whole check may take.
pub fn time_limit(limit: Duration) -> Options {
    Options::default().time_limit(limit)
}

/// Returns the settings with the partitions that the check searches at
/// once.
///
/// # Panics
///
/// Panics for 0 workers.
pub fn workers(n: usize) -> Options {
    Options::default().workers(n)
}
