// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The case: one call of a property's body, and the seat of its assertions.
//!
//! A case keeps each failure record in call order, and its first record is
//! the failure of the case. A failure on `check` ends the case. `assume`, a
//! draw past the cap and a draw that the trace of the run refuses end it too.
//! Every later call of the case ends it again, so a body that catches the
//! end of its case cannot run past it.
//!
//! However the body ends, the case runs its cleanups, the last registered
//! first. A failure in a cleanup fails the case as a failure in the body
//! does, and the later cleanups still run.

use super::engine::case::{
    Drawn, Erased, Failed, Mark, Record, Rendered, Request, Signal, Stop, identity_key,
};
use super::engine::choice::{Choice, IntegerBounds, SequenceBounds, UINT64_MAX};
use super::generator::Generator;
use crate::cancel::Cancel;
use crate::clock::Clock;
use crate::failure::{Failure, Where};
use crate::history::History;
use crate::record::Calls;
use crate::seat::Seat;
use crate::value::Value;
use std::error::Error;
use std::fmt::{self, Debug};
use std::panic::{Location, resume_unwind};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

fn locked<T>(lock: &Mutex<T>) -> MutexGuard<'_, T> {
    lock.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The seat of a property, as each of its cases reports to it.
pub(crate) struct Context<'s> {
    /// The seat of the property.
    pub(crate) parent: &'s dyn Seat,
    /// The call site of the property.
    pub(crate) site: Option<Where>,
    /// Whether every record of a case is at the call site of the property:
    /// a property form calls its assertion from inside the library.
    pub(crate) pinned: bool,
    /// Whether the call of the property is recorded, so that each case
    /// records the calls of its body.
    pub(crate) recorded: bool,
}

/// What a case has been told.
#[derive(Default)]
struct Reported {
    identities: Vec<Failed>,
    stopped: Option<Stop>,
    cleanups: Vec<Box<dyn FnOnce() + Send>>,
}

/// One call of a property's body.
///
/// The body draws its inputs from the case, and its assertions report to the
/// case, which is a [`Seat`]: every assertion of this library works on it.
/// A failure on `check` ends the case, and a failure on `soft` fails the case
/// when its body returns.
///
/// ```
/// use dokimi_assert::{Collector, check, prop};
///
/// let t = Collector::new();
/// prop::for_all(&t, "a sorted list is ordered", prop::cases(50), |c| {
///     let mut xs = c.draw(&prop::list(prop::integer(-100..=100), ..), "xs");
///     xs.sort_unstable();
///     check::pairwise(c, &xs, |a, b| a <= b, "sort orders the list");
/// });
/// ```
pub struct Case<'s> {
    record: Mutex<Record>,
    context: Option<&'s Context<'s>>,
    cancellation: Cancel,
    calls: Option<Arc<Calls>>,
    reported: Mutex<Reported>,
    /// The steps of the case's walk so far, which each call of the body
    /// records, so a walk that stops the case cuts the later calls.
    steps: Arc<AtomicUsize>,
    history: History,
}

impl Debug for Case<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Case")
            .field("record", &*locked(&self.record))
            .finish_non_exhaustive()
    }
}

/// The span that a decode opened, which closes when the guard drops, also
/// when the decode ends the case.
pub(crate) struct SpanGuard<'c, 's> {
    case: &'c Case<'s>,
    index: usize,
}

impl Drop for SpanGuard<'_, '_> {
    fn drop(&mut self) {
        self.case.with_record(|record| record.close(self.index));
    }
}

impl<'s> Case<'s> {
    /// Returns a case over `record`. A case of a property reports to
    /// `context`, and a case of the engine alone reports nowhere.
    pub(crate) fn new(record: Record, context: Option<&'s Context<'s>>) -> Self {
        let cancellation =
            context.map_or_else(Cancel::new, |context| context.parent.cancellation().child());
        let steps = Arc::new(AtomicUsize::new(0));
        let position = Arc::clone(&steps);
        let calls = context
            .filter(|context| context.recorded)
            .map(|_| Calls::running(Some(Arc::new(move || position.load(Ordering::Relaxed)))));
        Self {
            record: Mutex::new(record),
            context,
            cancellation,
            calls,
            reported: Mutex::default(),
            steps,
            history: History::new(),
        }
    }

    /// Runs `f` on the record of the case.
    pub(crate) fn with_record<R>(&self, f: impl FnOnce(&mut Record) -> R) -> R {
        f(&mut locked(&self.record))
    }

    /// Returns the record of the case, once its body has ended.
    pub(crate) fn into_parts(self) -> (Record, Option<Arc<Calls>>) {
        let record = self
            .record
            .into_inner()
            .unwrap_or_else(PoisonError::into_inner);
        (record, self.calls)
    }

    /// Returns the calls of the case's body, for a recorded property.
    pub(crate) fn calls_of(&self) -> Option<Arc<Calls>> {
        self.calls.clone()
    }

    /// Returns the signal that ended the case, once one has.
    pub(crate) fn check(&self) -> Result<(), Stop> {
        match &locked(&self.reported).stopped {
            Some(stop) => Err(stop.clone()),
            None => Ok(()),
        }
    }

    /// Ends the case with `stop`: keeps it as the signal that ends every later
    /// call, and unwinds the body.
    pub(crate) fn end(&self, stop: Stop) -> ! {
        {
            let mut reported = locked(&self.reported);
            if reported.stopped.is_none() {
                reported.stopped = Some(stop.clone());
            }
        }
        resume_unwind(Box::new(Signal(stop)))
    }

    /// Runs `f` on the record to make a choice, and keeps the steps of the
    /// walk.
    fn chosen<R>(&self, f: impl FnOnce(&mut Record) -> Result<R, Stop>) -> Result<R, Stop> {
        self.check()?;
        self.with_record(|record| {
            let chosen = f(record);
            self.steps.store(record.walk.len(), Ordering::Relaxed);
            chosen
        })
    }

    /// Returns the value for `request`, and records it.
    pub(crate) fn choose(&self, request: Request) -> Result<Choice, Stop> {
        self.chosen(|record| record.choose(request))
    }

    /// Returns an integer choice inside `bounds`.
    pub(crate) fn integer(
        &self,
        bounds: IntegerBounds,
        edge: Option<i128>,
        reuse: bool,
    ) -> Result<i128, Stop> {
        self.chosen(|record| record.integer(bounds, edge, reuse))
    }

    /// Returns a sequence choice inside `bounds`.
    pub(crate) fn sequence(&self, bounds: SequenceBounds) -> Result<Vec<u64>, Stop> {
        self.chosen(|record| record.sequence(bounds))
    }

    /// Opens a span labelled `label` that starts at `start`, or at the
    /// current index, and closes it when the guard drops.
    pub(crate) fn span(&self, label: &'static str, start: Option<usize>) -> SpanGuard<'_, 's> {
        let index = self.with_record(|record| record.open(label, start));
        SpanGuard { case: self, index }
    }

    /// Returns the number of choices that the record contains.
    pub(crate) fn len(&self) -> usize {
        self.with_record(|record| record.choices.len())
    }

    /// Returns the current point of the record, for a rewind.
    pub(crate) fn mark(&self) -> Mark {
        self.with_record(|record| record.mark())
    }

    /// Removes everything recorded after `mark`.
    pub(crate) fn rewind(&self, mark: Mark) {
        self.with_record(|record| record.rewind(mark));
    }

    /// Returns a value of `generator`, and records it under `label`.
    pub(crate) fn try_draw<T: Debug + 'static>(
        &self,
        generator: &Generator<T>,
        label: &str,
    ) -> Result<T, Stop> {
        self.check()?;
        let stated = self.with_record(|record| record.provider().valued());
        if let Some(stated) = stated {
            // A form states the values of its inputs, which it draws with one
            // generator of their type.
            let value = *stated
                .downcast::<T>()
                .expect("a form states values of the type of its inputs");
            let rendered = generator.render(&value);
            let erased: Arc<dyn Erased> = Arc::new(generator.clone());
            self.with_record(|record| {
                let span = record.spans.len();
                record.draws.push(Drawn {
                    label: label.to_owned(),
                    value: rendered,
                    span,
                    generator: erased,
                });
            });
            return Ok(value);
        }
        let traced = self.with_record(|record| {
            record
                .provider()
                .tracing()
                .map(|tracing| tracing.drawing(label, &|value| generator.invert(value)))
        });
        if let Some(Err(refused)) = traced {
            return Err(Stop::Trace(refused));
        }
        let span = self.with_record(|record| record.begin_draw(label));
        let decoded = generator.decode(self);
        self.with_record(Record::end_draw);
        let value = decoded?;
        let rendered = generator.render(&value);
        let erased: Arc<dyn Erased> = Arc::new(generator.clone());
        self.with_record(|record| {
            record.draws.push(Drawn {
                label: label.to_owned(),
                value: rendered,
                span,
                generator: erased,
            });
        });
        Ok(value)
    }

    /// Returns a value of `generator`, and records it under `label` for the
    /// report of a failing case.
    ///
    /// Two draws may share a label. A draw that the case cannot make ends the
    /// case: one that the case's cap on choices does not allow, one of a
    /// filter that keeps none of its attempts, and one that a stated trace
    /// refuses.
    pub fn draw<T: Debug + 'static>(&self, generator: &Generator<T>, label: &str) -> T {
        match self.try_draw(generator, label) {
            Ok(value) => value,
            Err(stop) => self.end(stop),
        }
    }

    /// Rejects the case when `condition` is `false`. A rejected case is
    /// neither counted nor shrunk, and a run that rejects ten times as many
    /// cases as it keeps fails.
    pub fn assume(&self, condition: bool) {
        if let Err(stop) = self.check() {
            self.end(stop);
        }
        if !condition {
            self.end(Stop::Rejected);
        }
    }

    /// Counts the case under `label`, which a requirement of the run reads. A
    /// label counted twice counts once.
    pub fn classify(&self, label: &str) {
        self.with_record(|record| {
            record.labels.insert(label.to_owned());
        });
    }

    /// Attaches `message` to the case. Only the report of a failing case
    /// states it.
    pub fn note(&self, message: &str) {
        self.with_record(|record| record.notes.push(message.to_owned()));
    }

    /// Records a fingerprint of the subject's state. The replay of a failing
    /// case compares the fingerprints, and a replay that observes another one
    /// makes the run flaky.
    pub fn observe(&self, fingerprint: u64) {
        self.with_record(|record| record.observe(fingerprint));
    }

    /// Records a score under `label`. The campaign profile favours the cases
    /// with the highest scores. A case keeps the higher of two scores under
    /// one label.
    pub fn target(&self, label: &str, score: f64) {
        self.with_record(|record| record.target(label, score));
    }

    /// Registers `f` to run when the case ends, after the body, however it
    /// ends. The last registered cleanup runs first.
    pub fn cleanup(&self, f: impl FnOnce() + Send + 'static) {
        locked(&self.reported).cleanups.push(Box::new(f));
    }

    /// Returns a source of random values whose every value is a choice of the
    /// case, so it replays and shrinks with the case.
    #[must_use]
    pub fn rand(&self) -> Rand<'_, 's> {
        Rand { case: self }
    }

    /// Returns the history of the case, empty when the case starts, into
    /// which the actions of a machine record their calls. The steps of a
    /// machine check it after each step.
    #[must_use]
    pub fn history(&self) -> &History {
        &self.history
    }

    /// Returns an integer choice over the whole unsigned 64-bit range.
    fn random(&self) -> u64 {
        match self.integer(IntegerBounds::of(0, UINT64_MAX), None, false) {
            Ok(value) => u64::try_from(value).unwrap_or_default(),
            Err(stop) => self.end(stop),
        }
    }

    /// Takes the cleanups that have not run, the last registered first.
    pub(crate) fn take_cleanups(&self) -> Vec<Box<dyn FnOnce() + Send>> {
        let mut cleanups = std::mem::take(&mut locked(&self.reported).cleanups);
        cleanups.reverse();
        cleanups
    }

    /// Stops the case's cancellation handle, as the end of the case does.
    pub(crate) fn finish(&self) {
        self.cancellation.cancel();
    }

    /// Returns the failure of the case: the identity of its first record, or
    /// `None` when nothing failed.
    pub(crate) fn failure(&self) -> Option<Failed> {
        locked(&self.reported).identities.first().cloned()
    }

    /// Returns the location of a record that the case's caller makes at
    /// `at`: the property's call site for a pinned case.
    fn locate(&self, at: Where) -> Option<Where> {
        match self.context {
            Some(context) if context.pinned => context.site,
            _ => Some(at),
        }
    }

    /// Keeps a failure, and returns the failure of the case: the first that
    /// it kept.
    pub(crate) fn keep(&self, failed: Failed) -> Failed {
        let mut reported = locked(&self.reported);
        reported.identities.push(failed);
        reported.identities[0].clone()
    }

    /// Keeps the failure of a panic of the body at `at`, with its message.
    pub(crate) fn panicked(&self, message: &str, at: Option<Where>) -> Failed {
        let at = at.or_else(|| self.context.and_then(|context| context.site));
        self.keep(Failed {
            identity: identity_key(None, None, Some("panic"), at),
            record: Some(Box::new(Failure::message(message, at))),
            entry: entry_of(None, None, Some("panic"), at),
        })
    }
}

/// Returns the failure of a record, with its identity and the identity's
/// form in a store entry.
///
/// A record of an assertion with a call site is kept by the assertion and the
/// call site, and one without a call site by the assertion and its contract.
/// A message to the case is kept by its call site.
fn failed_of(record: Failure) -> Failed {
    let assertion = record.assertion();
    let at = record.location();
    let contract = (assertion.is_some() && at.is_none()).then(|| record.contract());
    Failed {
        identity: identity_key(assertion, contract, None, at),
        entry: entry_of(assertion, contract, None, at),
        record: Some(Box::new(record)),
    }
}

/// Returns an identity in the form of a store entry: the assertion, the
/// contract or the error, beside the base name of the file and the line.
fn entry_of(
    assertion: Option<&str>,
    contract: Option<&str>,
    error: Option<&str>,
    at: Option<Where>,
) -> Vec<(&'static str, Value)> {
    let mut entry = Vec::new();
    if let Some(assertion) = assertion {
        entry.push(("assertion", Value::from(assertion)));
    }
    if let Some(contract) = contract {
        entry.push(("contract", Value::from(contract)));
    }
    if let Some(error) = error {
        entry.push(("error", Value::from(error)));
    }
    if let Some(at) = at {
        entry.push(("file", Value::from(at.base_name())));
        entry.push(("line", Value::from(at.line())));
    }
    entry
}

impl Seat for Case<'_> {
    #[track_caller]
    fn fail(&self, message: &str) {
        if let Err(stop) = self.check() {
            self.end(stop);
        }
        let at = self.locate(Where::from(Location::caller()));
        let failed = self.keep(failed_of(Failure::message(message, at)));
        self.end(Stop::Failed(Box::new(failed)));
    }

    #[track_caller]
    fn record(&self, message: &str) {
        if let Err(stop) = self.check() {
            self.end(stop);
        }
        let at = self.locate(Where::from(Location::caller()));
        self.keep(failed_of(Failure::message(message, at)));
    }

    #[track_caller]
    fn report(&self, failure: &Failure, aborting: bool) {
        if let Err(stop) = self.check() {
            self.end(stop);
        }
        let mut failure = failure.clone();
        if let Some(context) = self.context.filter(|context| context.pinned) {
            failure = failure.at(context.site);
        }
        let failed = self.keep(failed_of(failure));
        if aborting {
            self.end(Stop::Failed(Box::new(failed)));
        }
    }

    /// Ends the case at a fault that ends its call, as [`Seat::fail`] ends it
    /// with the text of the fault. A noted fault passes to the seat of the
    /// property, and the body runs on. A case of no property prints a noted
    /// fault to the standard error.
    #[track_caller]
    fn report_fault(&self, fault: Arc<dyn Error + Send + Sync>, ending: bool) {
        if ending {
            self.fail(&fault.to_string());
            return;
        }
        match self.context {
            Some(context) => context.parent.report_fault(fault, false),
            None => eprintln!("{fault}"),
        }
    }

    fn clock(&self) -> &dyn Clock {
        match self.context {
            Some(context) => context.parent.clock(),
            None => &crate::clock::SHARED,
        }
    }

    fn cancellation(&self) -> &Cancel {
        &self.cancellation
    }

    fn calls(&self) -> Option<Arc<Calls>> {
        self.calls.clone()
    }
}

/// A source of random values whose every value is a choice of its case.
///
/// [`Rand::next_u64`] takes one integer choice over the whole unsigned 64-bit
/// range, so a value that a body takes from it replays and shrinks with the
/// case. With the feature `rand_core`, it implements `rand_core::TryRng`
/// without an error, and with it `rand_core::Rng`, so the methods of
/// `rand::RngExt` take it.
pub struct Rand<'c, 's> {
    case: &'c Case<'s>,
}

impl Debug for Rand<'_, '_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Rand").finish_non_exhaustive()
    }
}

impl Rand<'_, '_> {
    /// Returns the next 64 random bits: one choice of the case.
    pub fn next_u64(&mut self) -> u64 {
        self.case.random()
    }

    /// Returns the next 32 random bits: the high half of one choice.
    pub fn next_u32(&mut self) -> u32 {
        u32::try_from(self.case.random() >> 32).unwrap_or_default()
    }

    /// Fills `dest` with random bytes, eight from each choice, in
    /// little-endian order.
    pub fn fill_bytes(&mut self, dest: &mut [u8]) {
        for chunk in dest.chunks_mut(8) {
            let bytes = self.case.random().to_le_bytes();
            chunk.copy_from_slice(&bytes[..chunk.len()]);
        }
    }
}

#[cfg(feature = "rand_core")]
impl rand_core::TryRng for Rand<'_, '_> {
    type Error = std::convert::Infallible;

    fn try_next_u32(&mut self) -> Result<u32, Self::Error> {
        Ok(self.next_u32())
    }

    fn try_next_u64(&mut self) -> Result<u64, Self::Error> {
        Ok(self.next_u64())
    }

    fn try_fill_bytes(&mut self, dst: &mut [u8]) -> Result<(), Self::Error> {
        self.fill_bytes(dst);
        Ok(())
    }
}

/// Returns the draw's value as a report shows it, from its generator.
pub(crate) fn rendered<T: Debug>(value: &T, literal: Option<Value>) -> Rendered {
    Rendered {
        text: format!("{value:?}"),
        literal,
    }
}
