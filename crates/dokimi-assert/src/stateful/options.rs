//! The settings of a run of steps: a builder whose methods each state one
//! setting.

use super::scheduler::{Scheduler, Section};
use std::fmt;

/// The settings of a run of [`steps`](super::steps).
///
/// Each method states one setting and returns the settings, so a caller
/// chains them. A free function of the same name, such as [`mean`], starts
/// the chain, and `Default::default()` states no setting.
#[derive(Clone, Copy)]
#[must_use]
pub struct Options<'s> {
    pub(crate) mean: usize,
    pub(crate) max: usize,
    pub(crate) swarm: bool,
    pub(crate) clients: usize,
    pub(crate) concurrent: usize,
    pub(crate) scheduler: Option<&'s (dyn Section + Sync + 's)>,
    pub(crate) repeat: usize,
}

impl fmt::Debug for Options<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Options")
            .field("mean", &self.mean)
            .field("max", &self.max)
            .field("swarm", &self.swarm)
            .field("clients", &self.clients)
            .field("concurrent", &self.concurrent)
            .field("tasks", &self.scheduler.is_some())
            .field("repeat", &self.repeat)
            .finish()
    }
}

/// States a mean of 30 sequential steps, at most 100 sequential steps and
/// 100 steps of the drain, a swarm, one client, at most 16 steps of a
/// concurrent section, clients on threads and 4 runs of a case whose section
/// runs on threads.
impl Default for Options<'_> {
    fn default() -> Self {
        Self {
            mean: 30,
            max: 100,
            swarm: true,
            clients: 1,
            concurrent: 16,
            scheduler: None,
            repeat: 4,
        }
    }
}

impl<'s> Options<'s> {
    /// States the mean number of sequential steps of a case.
    pub fn mean(mut self, n: usize) -> Self {
        self.mean = n;
        self
    }

    /// States the most sequential steps of a case, and the most steps of its
    /// drain.
    pub fn max(mut self, n: usize) -> Self {
        self.max = n;
        self
    }

    /// States whether a case first chooses the actions that it keeps. Off, a
    /// case keeps every action.
    pub fn swarm(mut self, on: bool) -> Self {
        self.swarm = on;
        self
    }

    /// States the clients of the concurrent section besides client 0. A
    /// section runs for `n` of 2 or more, on the clients 0 to `n`.
    ///
    /// # Panics
    ///
    /// Panics for `n` of 0.
    pub fn clients(mut self, n: usize) -> Self {
        assert!(n >= 1, "stateful: clients(0) is no count of 1 or more");
        self.clients = n;
        self
    }

    /// States the most steps of a concurrent section.
    pub fn concurrent(mut self, n: usize) -> Self {
        self.concurrent = n;
        self
    }

    /// Makes the clients 1 to `n` of a concurrent section run as tasks of
    /// `scheduler`, whose releases are choices of the case, so a race replays
    /// and shrinks. Without it, they run on threads.
    pub fn tasks(mut self, scheduler: &'s Scheduler<'_>) -> Self {
        self.scheduler = Some(scheduler);
        self
    }

    /// States the runs of a case whose concurrent section runs on threads.
    /// Threads do not replay, so such a case runs up to `n` times.
    ///
    /// # Panics
    ///
    /// Panics for `n` of 0.
    pub fn repeat(mut self, n: usize) -> Self {
        assert!(n >= 1, "stateful: repeat(0) is no count of 1 or more");
        self.repeat = n;
        self
    }
}

/// Returns the settings with the mean number of sequential steps.
pub fn mean<'s>(n: usize) -> Options<'s> {
    Options::default().mean(n)
}

/// Returns the settings with the most sequential steps and drain steps.
pub fn max<'s>(n: usize) -> Options<'s> {
    Options::default().max(n)
}

/// Returns the settings with the swarm on or off.
pub fn swarm<'s>(on: bool) -> Options<'s> {
    Options::default().swarm(on)
}

/// Returns the settings with the clients of the concurrent section.
///
/// # Panics
///
/// Panics for `n` of 0.
pub fn clients<'s>(n: usize) -> Options<'s> {
    Options::default().clients(n)
}

/// Returns the settings with the most steps of a concurrent section.
pub fn concurrent<'s>(n: usize) -> Options<'s> {
    Options::default().concurrent(n)
}

/// Returns the settings whose concurrent clients run as tasks of
/// `scheduler`.
pub fn tasks<'s>(scheduler: &'s Scheduler<'_>) -> Options<'s> {
    Options::default().tasks(scheduler)
}

/// Returns the settings with the runs of a case whose section runs on
/// threads.
///
/// # Panics
///
/// Panics for `n` of 0.
pub fn repeat<'s>(n: usize) -> Options<'s> {
    Options::default().repeat(n)
}
