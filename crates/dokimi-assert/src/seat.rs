// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The seats that assertions report to, and the fault that ends the call of
//! a helper.

use crate::cancel::Cancel;
use crate::clock::{Clock, SHARED};
use crate::failure::{Failure, Where};
use crate::json::{self, Json};
use crate::matcher::panics::{last_location, message, quietly};
use crate::record::{self, Calls, Switch};
use crate::verdict::{self, Fault};
use std::error::Error;
use std::fmt::{self, Write as _};
use std::panic::Location;
use std::sync::{Arc, LazyLock, Mutex, MutexGuard, PoisonError};

/// The handle of every seat that supplies none, which never stops.
static NEVER: LazyLock<Cancel> = LazyLock::new(Cancel::new);

/// Receives the failures of the assertions that a test calls.
///
/// An assertion never calls a test framework, and never panics on its own.
/// It reports to the seat that the test hands it, so one assertion serves a
/// real test, a benchmark and a test that checks the assertion itself. Each
/// seat decides what a failure does:
///
/// | Seat | A failure on `check` | A failure on `soft` |
/// |---|---|---|
/// | [`Collector`] | Panics | Is collected, and the drop of the collector panics with every one |
/// | [`Standard`] | Panics | Panics |
/// | [`Recorder`] | Is kept | Is kept |
///
/// A seat is [`Send`] and [`Sync`], because an assertion can report from a
/// thread that the test started, and one test hands one seat to many calls.
///
/// A seat that implements `fail` and `record` alone gets the platform clock,
/// a cancellation handle that never stops, the rendered sentence of each
/// failure, and a `cleanup` that ends the call with a fault:
///
/// ```
/// use dokimi_assert::{Seat, check};
/// use std::sync::Mutex;
///
/// #[derive(Default)]
/// struct Log(Mutex<Vec<String>>);
///
/// impl Seat for Log {
///     fn fail(&self, message: &str) {
///         self.0.lock().unwrap().push(format!("fail: {message}"));
///     }
///     fn record(&self, message: &str) {
///         self.0.lock().unwrap().push(format!("record: {message}"));
///     }
/// }
///
/// let log = Log::default();
/// check::is_true(&log, false, "the flag is set");
/// assert_eq!(*log.0.lock().unwrap(), ["fail: the flag is set"]);
/// ```
pub trait Seat: Send + Sync {
    /// Marks the calling frame as a frame of a helper, as Go's
    /// `testing.TB.Helper` does.
    ///
    /// The default does nothing. `#[track_caller]` already reports the line
    /// of the caller of every assertion.
    fn helper(&self) {}

    /// Reports a failure that ends the test.
    ///
    /// A seat that panics here ends the test at the caller's line.
    #[track_caller]
    fn fail(&self, message: &str);

    /// Reports a failure after which the test continues.
    #[track_caller]
    fn record(&self, message: &str);

    /// Receives the record of a failure.
    ///
    /// The default renders the record and calls [`Seat::fail`] when
    /// `aborting` is `true`, and [`Seat::record`] otherwise. A seat that keeps
    /// records overrides it.
    #[track_caller]
    fn report(&self, failure: &Failure, aborting: bool) {
        let sentence = failure.to_string();
        if aborting {
            self.fail(&sentence);
        } else {
            self.record(&sentence);
        }
    }

    /// Receives a fault: an error that ends the call without a verdict when
    /// `ending` is `true`, and that the call notes and runs on past when it is
    /// `false`.
    ///
    /// A fault is an argument or an environment that a call refuses, such as
    /// a pattern that does not compile. The default sends the text of an
    /// ending fault through [`Seat::fail`], and prints the text of any other
    /// fault to the standard error. A seat that keeps faults overrides it.
    #[track_caller]
    fn report_fault(&self, fault: Arc<dyn Error + Send + Sync>, ending: bool) {
        if ending {
            self.fail(&fault.to_string());
        } else {
            eprintln!("{fault}");
        }
    }

    /// Skips the test of the seat with `reason`, where the test runner can
    /// skip a running test.
    ///
    /// The default writes the reason to the standard error and returns,
    /// because libtest has no call that skips a running test. On the seat
    /// that `rejects` hands its check, `skip` ends the check. On a case, it
    /// ends the run of the property. A caller returns after `skip`.
    fn skip(&self, reason: &str) {
        write_skip(reason);
    }

    /// Observes one assertion call, a passing one included.
    ///
    /// The seat receives each call that a recorded run records, whatever
    /// `DOKIMI_ASSERT_RECORD` states. It receives each call once, before the
    /// failure, the fault or the skip of that call. It receives the calls of a
    /// body when the call that ran the body takes them, so the calls of the
    /// cases of a property arrive in the order of a run on one worker. The
    /// default does nothing.
    fn observe_call(&self, call: &Call<'_>) {
        let _ = call;
    }

    /// Registers `f` to run when the test, the case or the body of the seat
    /// ends.
    ///
    /// A seat runs its cleanups the last registered first, however the test
    /// or the body ended. A [`Collector`] runs them when it drops, a
    /// [`Recorder`] at [`Recorder::run_cleanups`], a case when the case ends,
    /// and the seat that `rejects` or `eventually` hands a body when the body
    /// ends. A case and the seat of a body stop their cancellation handle
    /// before the first cleanup runs.
    ///
    /// The default ends the call with a fault, and `f` does not run.
    ///
    /// ```
    /// use dokimi_assert::{Recorder, Seat};
    /// use std::sync::Arc;
    /// use std::sync::atomic::{AtomicBool, Ordering};
    ///
    /// fn served(seat: &dyn Seat) -> Arc<AtomicBool> {
    ///     let running = Arc::new(AtomicBool::new(true));
    ///     let server = Arc::clone(&running);
    ///     seat.cleanup(Box::new(move || server.store(false, Ordering::SeqCst)));
    ///     running
    /// }
    ///
    /// let rec = Recorder::new();
    /// let running = served(&rec);
    /// rec.run_cleanups();
    /// assert!(!running.load(Ordering::SeqCst));
    /// ```
    #[track_caller]
    fn cleanup(&self, f: Box<dyn FnOnce() + Send>) {
        drop(f);
        let fault = Fault::new("Seat::cleanup: the seat does not run cleanups");
        self.report_fault(Arc::new(fault), true);
    }

    /// Returns the clock that the timed assertions read.
    ///
    /// The default is the platform clock.
    fn clock(&self) -> &dyn Clock {
        &SHARED
    }

    /// Returns the cancellation handle of the seat's test.
    ///
    /// The default is a handle that never stops, because libtest gives a test
    /// no handle. A body that a property, `rejects` or `eventually` runs gets
    /// a handle that derives from the handle of the assertion's seat, and the
    /// assertion stops it when the body ends.
    fn cancellation(&self) -> &Cancel {
        &NEVER
    }

    /// Returns the calls whose records the seat keeps or writes.
    #[doc(hidden)]
    fn calls(&self) -> Option<Arc<Calls>> {
        None
    }

    /// Returns the path of the test that the seat belongs to, whose store a
    /// property of the test keeps its failures in, or `None` for a seat of no
    /// test.
    #[doc(hidden)]
    fn test(&self) -> Option<String> {
        None
    }
}

/// An assertion call, with the id of the assertion and the contract.
///
/// [`Seat::observe_call`] receives one for each call:
///
/// ```
/// use dokimi_assert::{Call, Seat, check};
/// use std::sync::Mutex;
///
/// #[derive(Default)]
/// struct Handle(Mutex<Vec<String>>);
///
/// impl Seat for Handle {
///     fn fail(&self, message: &str) {
///         panic!("{message}");
///     }
///     fn record(&self, message: &str) {
///         panic!("{message}");
///     }
///     fn observe_call(&self, call: &Call<'_>) {
///         self.0.lock().unwrap().push(call.contract.to_owned());
///     }
/// }
///
/// let handle = Handle::default();
/// check::is_true(&handle, true, "the flag is set");
/// assert_eq!(*handle.0.lock().unwrap(), ["the flag is set"]);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Call<'c> {
    /// The id of the assertion, as the definition names it.
    pub assertion: &'static str,
    /// The message of the caller, unchanged.
    pub contract: &'c str,
}

/// Ends the call of a helper with `error`, a fault.
///
/// A fault is an argument or an environment that the helper refuses. The
/// seat receives `error` through [`Seat::report_fault`] with `ending` true:
/// a [`Recorder`] keeps it, and a [`Collector`] or a [`Standard`] panics with
/// its text. The faults of this library end their calls the same way. `fault`
/// does not write a call record, because the call is not a call of an
/// assertion. A seat that keeps the fault returns, so the helper returns after
/// `fault`:
///
/// ```
/// use dokimi_assert::{Recorder, Seat, fault};
/// use std::io;
///
/// fn restart(seat: &dyn Seat, restartable: bool) {
///     if !restartable {
///         fault(seat, io::Error::other("the service cannot restart"));
///         return;
///     }
///     // …
/// }
///
/// let r = Recorder::new();
/// restart(&r, false);
/// assert_eq!(r.faults()[0].to_string(), "the service cannot restart");
/// assert_eq!(r.message(), "the service cannot restart");
/// ```
#[track_caller]
pub fn fault(seat: &dyn Seat, error: impl Error + Send + Sync + 'static) {
    seat.helper();
    seat.report_fault(Arc::new(error), true);
}

/// The deepest nesting of a record that [`write_record`] accepts.
const RECORD_DEPTH: usize = 512;

/// Writes `record`, the JSON text of one record of `kind`, to the artifact of
/// the test of `seat`.
///
/// The artifact of a kind is a file beside the test binary. Its name is the
/// name of the binary, the kind with each dot replaced by a dash, and
/// `.jsonl`, so the records of `dokimi.suite` are in
/// `<binary>.dokimi-suite.jsonl`. The first record of a kind in a run of the
/// binary creates the file anew. Each record is one line of the file, so
/// `write_record` replaces each line break of `record` with a space. The JSON
/// value of the record stays the same. A tool of dokimasia writes its own
/// records beside the call records this way, and writes the name of the test
/// into a field of its record:
///
/// ```
/// use dokimi_assert::{Collector, write_record};
///
/// let t = Collector::new();
/// write_record(&t, "dokimi.suite", r#"{"test":"ledger::suite","seed":"7"}"#);
/// ```
///
/// A [`Collector`] and a [`Standard`] write the record in a test that libtest
/// runs. A [`Recorder`], the case of a property, the seat that `rejects` or
/// `eventually` hands a body and a seat outside a test do not write it.
/// `write_record` ignores `DOKIMI_ASSERT_RECORD`.
///
/// `write_record` ends the call with a fault, as [`fault`] ends one, and does
/// not write the record in these cases:
///
/// - `kind` does not match `dokimi\.[a-z][a-z0-9-]*`.
/// - `kind` is `dokimi.assert`, the kind of the call records of this crate.
/// - `record` is not the JSON text of one object. The JSON reader of this
///   crate also refuses a name that repeats within an object, and a nesting
///   deeper than 512 levels.
#[track_caller]
pub fn write_record(seat: &dyn Seat, kind: &str, record: &str) {
    seat.helper();
    if let Some(refused) = refusal(kind, record) {
        verdict::end(seat, format_args!("write_record: {refused}"));
        return;
    }
    if seat.test().is_some() {
        record::artifact::write(kind, &record.replace(['\r', '\n'], " "));
    }
}

/// Returns the fault of a kind or a record that [`write_record`] refuses, and
/// `None` for a kind and a record that it writes.
fn refusal(kind: &str, record: &str) -> Option<String> {
    if !is_kind(kind) {
        return Some(format!(
            r"kind: {kind:?} does not match dokimi\.[a-z][a-z0-9-]*"
        ));
    }
    if kind == record::CALL_KIND {
        return Some(format!(
            "kind: {kind} is the kind of the call records of assert"
        ));
    }
    match json::parse(record, RECORD_DEPTH) {
        Ok(Json::Object(_)) => None,
        _ => Some("record: the record is not one JSON object".to_owned()),
    }
}

/// Returns `true` when `kind` matches `dokimi\.[a-z][a-z0-9-]*`.
fn is_kind(kind: &str) -> bool {
    let Some(rest) = kind.strip_prefix("dokimi.") else {
        return false;
    };
    let mut bytes = rest.bytes();
    bytes.next().is_some_and(|first| first.is_ascii_lowercase())
        && bytes.all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

/// Writes the reason of a skip to the standard error, as the default of
/// [`Seat::skip`] does.
pub(crate) fn write_skip(reason: &str) {
    eprintln!("skipped: {reason}");
}

/// Returns the calls of a seat of a test: calls that write each record to the
/// artifact while the switch is on, and `None` otherwise.
fn test_calls(test: Option<String>) -> Option<Arc<Calls>> {
    (*record::switch() == Switch::On).then(|| Calls::writing(test))
}

/// Returns the name of the test that the current thread runs.
fn thread_test() -> String {
    std::thread::current().name().unwrap_or_default().to_owned()
}

/// Returns the path of a test, or `None` for a thread that runs no test:
/// one without a name, and the main thread of a program, which also runs
/// each doctest.
fn named(test: &str) -> Option<String> {
    (!test.is_empty() && test != "main").then(|| test.to_owned())
}

fn locked<T>(lock: &Mutex<T>) -> MutexGuard<'_, T> {
    lock.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A function that a seat runs when its test, its case or its body ends.
pub(crate) type Cleanup = Box<dyn FnOnce() + Send>;

/// The cleanups that a seat keeps until its test, its case or its body ends.
#[derive(Default)]
pub(crate) struct Cleanups(Mutex<Vec<Cleanup>>);

impl fmt::Debug for Cleanups {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Cleanups")
            .field(&locked(&self.0).len())
            .finish()
    }
}

impl Cleanups {
    /// Keeps `cleanup` until the next run.
    pub(crate) fn add(&self, cleanup: Cleanup) {
        locked(&self.0).push(cleanup);
    }

    /// Passes each kept cleanup to `each`, the last kept first, and forgets
    /// it. A cleanup that a cleanup keeps runs in the same pass.
    pub(crate) fn run(&self, mut each: impl FnMut(Cleanup)) {
        loop {
            let next = locked(&self.0).pop();
            let Some(cleanup) = next else {
                return;
            };
            each(cleanup);
        }
    }
}

/// A seat that panics on every failure.
///
/// The panic is how libtest reads a failure, so this is the seat for code
/// that has no seat of its own, such as a fuzz target. A failure on `soft`
/// panics too: a collected failure needs an end of the test to report at,
/// and this seat has none.
#[derive(Debug, Clone, Copy, Default)]
pub struct Standard;

impl Standard {
    /// Returns the seat.
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

    fn calls(&self) -> Option<Arc<Calls>> {
        static CALLS: LazyLock<Option<Arc<Calls>>> = LazyLock::new(|| test_calls(None));
        CALLS.clone()
    }

    fn test(&self) -> Option<String> {
        named(&thread_test())
    }
}

/// What a recorder has been told.
#[derive(Debug, Default)]
struct Reported {
    fatal: Option<String>,
    recorded: Vec<String>,
    failures: Vec<Failure>,
    faults: Vec<Arc<dyn Error + Send + Sync>>,
    helpers: usize,
}

/// A seat that keeps every failure and panics on none.
///
/// A test drives an assertion against a recorder and reads what the
/// assertion reported, rather than being stopped by it. Nothing that runs on
/// a recorder fails the test.
///
/// A recorder keeps the cleanups that [`Seat::cleanup`] registers until
/// [`Recorder::run_cleanups`] runs them, so a test of a helper can check what
/// the cleanup of the helper does. The drop of the recorder runs the cleanups
/// that remain.
///
/// ```
/// use dokimi_assert::{Recorder, check};
///
/// let r = Recorder::new();
/// check::has_prefix(&r, "GET /", "POST", "the method is POST");
/// assert!(r.failed());
/// assert_eq!(r.failures()[0].assertion(), Some("has-prefix"));
/// assert_eq!(r.records().len(), 1);
/// ```
#[derive(Debug)]
pub struct Recorder {
    reported: Mutex<Reported>,
    clock: Option<Arc<dyn Clock>>,
    cancellation: Cancel,
    calls: Option<Arc<Calls>>,
    cleanups: Cleanups,
}

impl Default for Recorder {
    fn default() -> Self {
        Self::new()
    }
}

impl Recorder {
    /// Returns a recorder that has kept nothing.
    #[must_use]
    pub fn new() -> Self {
        Self {
            reported: Mutex::default(),
            clock: None,
            cancellation: Cancel::new(),
            calls: Some(Calls::keeping()),
            cleanups: Cleanups::default(),
        }
    }

    /// Returns the recorder with `clock` as the clock that the timed
    /// assertions read.
    #[must_use]
    pub fn with_clock(mut self, clock: Arc<dyn Clock>) -> Self {
        self.clock = Some(clock);
        self
    }

    /// Returns the recorder with `cancellation` as its cancellation handle.
    #[must_use]
    pub fn with_cancellation(mut self, cancellation: Cancel) -> Self {
        self.cancellation = cancellation;
        self
    }

    /// Returns `true` when any failure was reported, on either surface.
    #[must_use]
    pub fn failed(&self) -> bool {
        let held = locked(&self.reported);
        held.fatal.is_some() || !held.recorded.is_empty()
    }

    /// Returns the first failure that ended the test, or else the first
    /// failure after which the test continued, and the empty string when
    /// nothing failed.
    #[must_use]
    pub fn message(&self) -> String {
        let held = locked(&self.reported);
        held.fatal
            .clone()
            .or_else(|| held.recorded.first().cloned())
            .unwrap_or_default()
    }

    /// Returns every failure after which the test continued, in the order of
    /// the calls.
    #[must_use]
    pub fn messages(&self) -> Vec<String> {
        locked(&self.reported).recorded.clone()
    }

    /// Returns the record of every failure, in the order of the calls.
    ///
    /// A message passed to [`Seat::fail`] or [`Seat::record`] directly leaves
    /// no record, so an assertion that reported no record is visible here.
    #[must_use]
    pub fn failures(&self) -> Vec<Failure> {
        locked(&self.reported).failures.clone()
    }

    /// Returns every fault that [`Seat::report_fault`] received, in the order
    /// of the calls: the faults that ended their calls and the faults that a
    /// call noted.
    ///
    /// A test compares a fault as the error it is, or compares its text. It
    /// does not search the message of the recorder for it.
    #[must_use]
    pub fn faults(&self) -> Vec<Arc<dyn Error + Send + Sync>> {
        locked(&self.reported).faults.clone()
    }

    /// Returns how many times [`Seat::helper`] was called.
    #[must_use]
    pub fn helper_calls(&self) -> usize {
        locked(&self.reported).helpers
    }

    /// Returns the call record of every call on the recorder, one line of
    /// JSON each, in the order of their numbers.
    ///
    /// The recorder keeps the records whatever `DOKIMI_ASSERT_RECORD` states.
    /// A call that runs a body, such as `eventually`, states the calls of its
    /// body after its own record, each with the number of its parent.
    #[must_use]
    pub fn records(&self) -> Vec<String> {
        self.calls
            .as_ref()
            .map(|calls| calls.lines())
            .unwrap_or_default()
    }

    /// Runs the cleanups that the recorder keeps, the last registered first,
    /// and forgets them.
    ///
    /// A cleanup that a cleanup registers runs before the call returns, and a
    /// cleanup that arrives after the call waits for the next call. A cleanup
    /// that panics fails the recorder with the failure `a cleanup failed: `
    /// and the text of the panic. The later cleanups still run.
    pub fn run_cleanups(&self) {
        self.cleanups.run(|cleanup| {
            if let Err(raised) = quietly(cleanup) {
                self.record(&format!("a cleanup failed: {}", message(&*raised)));
            }
        });
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        self.run_cleanups();
    }
}

impl Seat for Recorder {
    fn helper(&self) {
        locked(&self.reported).helpers += 1;
    }

    #[track_caller]
    fn fail(&self, message: &str) {
        let mut held = locked(&self.reported);
        if held.fatal.is_none() {
            held.fatal = Some(message.to_owned());
        }
    }

    #[track_caller]
    fn record(&self, message: &str) {
        locked(&self.reported).recorded.push(message.to_owned());
    }

    #[track_caller]
    fn report(&self, failure: &Failure, aborting: bool) {
        locked(&self.reported).failures.push(failure.clone());
        let sentence = failure.to_string();
        if aborting {
            self.fail(&sentence);
        } else {
            self.record(&sentence);
        }
    }

    /// Keeps the fault. A fault that ends its call fails the recorder with
    /// the text of the fault, as [`Seat::fail`] does, and a noted fault does
    /// not fail it.
    #[track_caller]
    fn report_fault(&self, fault: Arc<dyn Error + Send + Sync>, ending: bool) {
        let text = ending.then(|| fault.to_string());
        locked(&self.reported).faults.push(fault);
        if let Some(text) = text {
            self.fail(&text);
        }
    }

    /// Keeps `f` until [`Recorder::run_cleanups`] or the drop of the
    /// recorder.
    fn cleanup(&self, f: Box<dyn FnOnce() + Send>) {
        self.cleanups.add(f);
    }

    fn clock(&self) -> &dyn Clock {
        match &self.clock {
            Some(clock) => clock.as_ref(),
            None => &SHARED,
        }
    }

    fn cancellation(&self) -> &Cancel {
        &self.cancellation
    }

    fn calls(&self) -> Option<Arc<Calls>> {
        self.calls.clone()
    }
}

/// A seat that panics on a failure of `check`, and collects a failure of
/// `soft` until the test ends.
///
/// This is the seat of a test. `check` stops the test at the first failure.
/// `soft` lets the test continue, and the drop of the collector at the end of
/// the test panics with every failure that it collected, each with the line
/// of its assertion:
///
/// ```should_panic
/// use dokimi_assert::{Collector, soft};
///
/// let t = Collector::new();
/// soft::is_true(&t, false, "the flag is set");
/// soft::equal(&t, 1, 2, "the count is right");
/// // The drop panics with "2 failures: …".
/// ```
///
/// The drop runs the cleanups that [`Seat::cleanup`] registered, the last
/// registered first, before it panics. A cleanup that panics adds the failure
/// `a cleanup failed: ` with the text of the panic, and the later cleanups
/// still run.
///
/// A collector that drops while its thread unwinds from another panic panics
/// with nothing, because a second panic would abort the process. It still runs
/// its cleanups, and it writes the failure of a cleanup that panics to the
/// standard error.
#[derive(Debug)]
pub struct Collector {
    collected: Mutex<Vec<(String, Where)>>,
    calls: Option<Arc<Calls>>,
    test: String,
    cleanups: Cleanups,
}

impl Default for Collector {
    fn default() -> Self {
        Self::new()
    }
}

impl Collector {
    /// Returns a collector that has collected nothing, as the seat of the
    /// test that the current thread runs.
    #[must_use]
    pub fn new() -> Self {
        let test = thread_test();
        Self {
            collected: Mutex::default(),
            calls: test_calls(Some(test.clone())),
            test,
            cleanups: Cleanups::default(),
        }
    }

    /// Returns the sentence of every failure collected so far, in the order of
    /// the calls.
    #[must_use]
    pub fn collected(&self) -> Vec<String> {
        locked(&self.collected)
            .iter()
            .map(|(message, _)| message.clone())
            .collect()
    }

    /// Panics with every failure collected so far, and keeps none of them.
    ///
    /// The drop of the collector calls it, so a test need not. A test calls it
    /// to report its soft failures at a point of its own choosing.
    ///
    /// # Panics
    ///
    /// Panics when the collector has collected a failure, with one line for
    /// each failure and its location.
    #[track_caller]
    pub fn flush(&self) {
        let taken = std::mem::take(&mut *locked(&self.collected));
        if taken.is_empty() {
            return;
        }
        panic!("{}", summary(&taken));
    }
}

/// Renders the collected failures as one report: a failure alone with its
/// location, and several as a numbered list.
fn summary(failures: &[(String, Where)]) -> String {
    if let [(only, at)] = failures {
        return format!("{only}\n  at {at}");
    }
    let mut out = format!("{} failures:", failures.len());
    for (index, (failure, at)) in failures.iter().enumerate() {
        // Writing to a String cannot fail.
        let _ = write!(out, "\n  {}. {failure}\n     at {at}", index + 1);
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
        // The caller's location is known here, and not at the drop.
        locked(&self.collected).push((message.to_owned(), Where::from(Location::caller())));
    }

    /// Keeps `f` until the drop of the collector.
    fn cleanup(&self, f: Box<dyn FnOnce() + Send>) {
        self.cleanups.add(f);
    }

    fn calls(&self) -> Option<Arc<Calls>> {
        self.calls.clone()
    }

    fn test(&self) -> Option<String> {
        named(&self.test)
    }
}

impl Drop for Collector {
    fn drop(&mut self) {
        let unwinding = std::thread::panicking();
        self.cleanups.run(|cleanup| {
            let Err(raised) = quietly(cleanup) else {
                return;
            };
            let failure = format!("a cleanup failed: {}", message(&*raised));
            if unwinding {
                eprintln!("{failure}");
                return;
            }
            let at = last_location().unwrap_or(Where::from(Location::caller()));
            locked(&self.collected).push((failure, at));
        });
        if !unwinding {
            self.flush();
        }
    }
}
