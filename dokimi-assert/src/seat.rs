//! Where an assertion reports, and what it may do about it.

use crate::clock::{Clock, System};
use crate::failure::Failure;
use std::fmt::Write as _;
use std::panic::Location;
use std::sync::Arc;
use std::sync::Mutex;

/// Where a failure goes, and what may be done about it.
///
/// An assertion never calls a test framework and never panics on its own. It reports
/// through a seat, which is what lets one assertion serve a real test, a benchmark and a
/// test that checks the assertion itself.
///
/// A trait rather than a concrete type, so anything with the three methods is a seat.
/// The methods take `&self` rather than `&mut self` because a test holds one seat and
/// hands it to every assertion in the body; a seat that collects keeps its own lock.
pub trait Seat {
    /// Mark this frame as the library's rather than the caller's.
    ///
    /// Named for the same reason Go's `testing.TB.Helper` is. A framework that can hide
    /// library frames from a backtrace does it here; one that cannot does nothing, which
    /// is why this has a default body.
    fn helper(&self) {}

    /// Report a failure that stops the test.
    #[track_caller]
    fn fail(&self, message: &str);

    /// Report a failure the test may carry on past.
    #[track_caller]
    fn record(&self, message: &str);

    /// Whether this seat takes records rather than only sentences.
    ///
    /// A seat that answers true has [`Seat::report`] called with the record; one that
    /// does not gets the sentence rendered from it through [`Seat::fail`] or
    /// [`Seat::record`]. Two methods rather than an `Option` return, because a trait
    /// object cannot be downcast without `Any` and the standard states this as a
    /// capability a seat either has or does not.
    fn takes_records(&self) -> bool {
        false
    }

    /// Take one record.
    ///
    /// Called only when [`Seat::takes_records`] answers true, so the default does
    /// nothing.
    #[track_caller]
    fn report(&self, _failure: &Failure, _aborting: bool) {}

    /// The clock assertions reported here read.
    ///
    /// A seat that supplies none gets the platform clock, which is what every assertion
    /// read before a clock existed.
    fn clock(&self) -> &dyn Clock {
        &SYSTEM
    }
}

/// The platform clock every seat reads unless it supplies another.
static SYSTEM: System = System::shared();

/// A seat that panics on either path.
///
/// The panic is what a Rust test harness reads as a failure, so this is the seat for
/// code that has no seat of its own. [`Soft`](crate::soft) panics through it too rather
/// than dropping the failure: a recorded failure needs somewhere to report at the end,
/// and a bare seat has no end to report at.
#[derive(Debug, Default)]
pub struct Standard;

impl Standard {
    /// Return a seat that panics on either path.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl Seat for Standard {
    #[track_caller]
    fn fail(&self, message: &str) {
        panic!("{message}");
    }

    #[track_caller]
    fn record(&self, message: &str) {
        panic!("{message}");
    }
}

/// What a seat has been told, kept behind one lock.
#[derive(Debug, Default)]
struct Reported {
    fatal: Option<String>,
    recorded: Vec<String>,
    records: Vec<Failure>,
    helpers: usize,
}

/// A seat that collects every failure and panics on none.
///
/// This is what lets an assertion be tested by reading what it reported rather than
/// suffering it. Nothing driven with a recorder can fail a test.
#[derive(Debug, Default)]
pub struct Recorder {
    reported: Mutex<Reported>,
    /// What assertions reported here read time from, or `None` for the platform clock.
    supplied: Option<Arc<dyn Clock>>,
}

impl Recorder {
    /// Every record that arrived, in call order.
    ///
    /// A message passed straight to `fail` or `record` leaves none, so an assertion that
    /// did not report a record is visible here.
    ///
    /// # Panics
    ///
    /// If another thread panicked while holding this seat's lock.
    #[must_use]
    pub fn failures(&self) -> Vec<Failure> {
        self.locked().records.clone()
    }

    /// Make assertions reported here read the given clock.
    #[must_use]
    pub fn with_clock(mut self, clock: Arc<dyn Clock>) -> Self {
        self.supplied = Some(clock);
        self
    }

    /// Return a recorder that has collected nothing.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether any failure was reported, through either path.
    #[must_use]
    pub fn failed(&self) -> bool {
        let held = self.locked();
        held.fatal.is_some() || !held.recorded.is_empty()
    }

    /// The first failure reported, preferring the aborting path.
    ///
    /// Empty when nothing failed. Reading this rather than indexing keeps a test from
    /// panicking when the assertion under test wrongly reported nothing.
    #[must_use]
    pub fn message(&self) -> String {
        let held = self.locked();
        held.fatal
            .clone()
            .or_else(|| held.recorded.first().cloned())
            .unwrap_or_default()
    }

    /// Every failure reported through [`Seat::record`], in call order.
    #[must_use]
    pub fn messages(&self) -> Vec<String> {
        self.locked().recorded.clone()
    }

    /// How many times [`Seat::helper`] was called.
    #[must_use]
    pub fn helper_calls(&self) -> usize {
        self.locked().helpers
    }

    /// Take the lock, recovering it when another thread panicked holding it.
    ///
    /// A poisoned lock means a subject panicked mid-assertion. What was reported before
    /// that is still what the test wants to read, and refusing to hand it back would
    /// replace the real failure with one about the lock.
    fn locked(&self) -> std::sync::MutexGuard<'_, Reported> {
        self.reported
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl Seat for Recorder {
    fn helper(&self) {
        self.locked().helpers += 1;
    }

    fn takes_records(&self) -> bool {
        true
    }

    /// Record one failure as the record it is.
    ///
    /// This is what lets a test read the assertion's own fields rather than search its
    /// sentence for words. The rendered sentence is kept too, so [`Recorder::message`]
    /// answers what it always did.
    #[track_caller]
    fn report(&self, failure: &Failure, aborting: bool) {
        self.locked().records.push(failure.clone());
        if aborting {
            self.fail(&failure.render());
            return;
        }
        self.record(&failure.render());
    }

    fn clock(&self) -> &dyn Clock {
        match &self.supplied {
            Some(held) => held.as_ref(),
            None => &SYSTEM,
        }
    }

    #[track_caller]
    fn fail(&self, message: &str) {
        let mut held = self.locked();
        if held.fatal.is_none() {
            held.fatal = Some(message.to_owned());
        }
    }

    #[track_caller]
    fn record(&self, message: &str) {
        self.locked().recorded.push(message.to_owned());
    }
}

/// A seat that panics on a check and collects a soft failure until the test ends.
///
/// This is what a real test wants. `check` stops at the first failure, `soft` carries on,
/// and everything soft collected is reported when the collector is dropped, so one run
/// shows every property that failed.
///
/// Dropping is what ends the test, so nothing has to be called at the end and nothing
/// can be forgotten. A collector already unwinding from another panic reports nothing:
/// panicking twice aborts the process, and the first failure is the one worth reading.
#[derive(Debug, Default)]
pub struct Collector {
    recorded: Mutex<Vec<(String, &'static Location<'static>)>>,
}

impl Collector {
    /// Return a collector that has collected nothing.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Every failure collected so far, in call order.
    #[must_use]
    pub fn collected(&self) -> Vec<String> {
        self.locked()
            .iter()
            .map(|(message, _)| message.clone())
            .collect()
    }

    /// Panic with everything collected, and keep nothing.
    ///
    /// Called on drop, so a test need not call it. Call it directly to end a test at a
    /// chosen point rather than at the end of the body.
    ///
    /// # Panics
    ///
    /// With a numbered report, when anything was collected.
    #[track_caller]
    pub fn flush(&self) {
        let taken = std::mem::take(&mut *self.locked());
        if taken.is_empty() {
            return;
        }
        panic!("{}", report(&taken));
    }

    fn locked(&self) -> std::sync::MutexGuard<'_, Vec<(String, &'static Location<'static>)>> {
        self.recorded
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// Render what a collector holds as one numbered failure.
///
/// Each line carries where the assertion was written. A collector reports from its drop,
/// which has no caller to attribute the failure to, so without this a reader is told only
/// that something failed somewhere in the test.
fn report(failures: &[(String, &'static Location<'static>)]) -> String {
    if let [(only, at)] = failures {
        return format!("{only}\n  at {}:{}", at.file(), at.line());
    }
    let mut out = format!("{} failures:", failures.len());
    for (index, (failure, at)) in failures.iter().enumerate() {
        let _ = write!(
            out,
            "\n  {}. {failure}\n     at {}:{}",
            index + 1,
            at.file(),
            at.line()
        );
    }
    out
}

impl Seat for Collector {
    #[track_caller]
    fn fail(&self, message: &str) {
        panic!("{message}");
    }

    #[track_caller]
    fn record(&self, message: &str) {
        // Taken here rather than at the drop, which is the only place
        // the caller is still known.
        self.locked().push((message.to_owned(), Location::caller()));
    }
}

impl Drop for Collector {
    fn drop(&mut self) {
        // Panicking while already unwinding aborts the process, which
        // would replace a readable failure with a core dump.
        if std::thread::panicking() {
            return;
        }
        self.flush();
    }
}
