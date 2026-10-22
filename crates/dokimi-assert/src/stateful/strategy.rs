// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The strategies of the task scheduler: how it chooses the ready task to
//! release.

/// How a [`Scheduler`](super::Scheduler) chooses the ready task to release.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Strategy {
    /// The depth of PCT, and 0 for the uniform strategy.
    pub(crate) depth: u64,
}

/// Returns the strategy that chooses each release among the ready tasks, in
/// the order that they became ready, uniformly, with target 0 and edge 0. A
/// release with one ready task records a choice that consumes nothing. A
/// shrunk schedule releases the tasks in the order that they became ready,
/// wherever the failure allows.
#[must_use]
pub fn uniform() -> Strategy {
    Strategy { depth: 0 }
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
    Strategy { depth }
}
