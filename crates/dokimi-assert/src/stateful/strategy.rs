// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The strategies of the task scheduler: how it chooses the ready task to
//! release, and how many tasks one call releases.

/// The most tasks that one run or one advance releases by default.
const MAX_RELEASES: usize = 1_000_000;

/// The rule by which a [`Scheduler`](super::Scheduler) chooses the ready task
/// to release, and the most tasks that one run or one advance of it releases.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Strategy {
    /// The depth of PCT, and 0 for the uniform strategy.
    pub(crate) depth: u64,
    /// The most tasks that one run or one advance releases.
    pub(crate) max_releases: usize,
}

impl Strategy {
    /// States the most tasks that one run or one advance releases,
    /// 1,000,000 by default. A run or an advance that has released `n` tasks
    /// while a task is still ready ends every task, and fails the case with
    /// the message "the task scheduler released n tasks in one run, and a
    /// task is still ready". Under the uniform strategy each release is a
    /// choice of the case, so a case passes the `max_choices` of the run
    /// first unless the run raises it.
    ///
    /// # Panics
    ///
    /// Panics for `n` of 0.
    #[must_use]
    pub fn max_releases(mut self, n: usize) -> Self {
        assert!(n >= 1, "stateful: max_releases(0) is no count of 1 or more");
        self.max_releases = n;
        self
    }
}

/// Returns the strategy that chooses each release among the ready tasks, in
/// the order that they became ready, uniformly, with target 0 and edge 0. A
/// release with one ready task records a choice that consumes nothing. A
/// shrunk schedule releases the tasks in the order that they became ready,
/// wherever the failure allows.
#[must_use]
pub fn uniform() -> Strategy {
    Strategy {
        depth: 0,
        max_releases: MAX_RELEASES,
    }
}

/// Returns the strategy of probabilistic concurrency testing of `depth`.
///
/// Each task receives a priority when it is spawned, a choice over
/// `[0, 2^64 − 1]` with target 0, and keeps it when it yields. The ready
/// task with the highest priority runs, the earliest ready among equals, and
/// a release makes no choice. Each run of the scheduler starts with
/// `depth − 1` change points, each a presence choice with edge 1 and then a
/// count of releases. The task released at that count falls below every
/// other task, and a later change point puts its task lower still.
///
/// # Panics
///
/// Panics for a depth of 0.
#[must_use]
pub fn pct(depth: u64) -> Strategy {
    assert!(depth >= 1, "stateful: pct(0) is no depth of 1 or more");
    Strategy {
        depth,
        max_releases: MAX_RELEASES,
    }
}

/// Returns the uniform strategy with the most tasks that one run or one
/// advance releases.
///
/// # Panics
///
/// Panics for `n` of 0.
#[must_use]
pub fn max_releases(n: usize) -> Strategy {
    uniform().max_releases(n)
}
