// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The call records of a recorded run.
//!
//! `DOKIMI_ASSERT_RECORD=1` makes the seat of a test write the call record
//! of every assertion call, pass or fail, to the artifact of the test
//! binary: a file beside the binary, named after it with the suffix
//! `.dokimi-assert.jsonl`. A recorder keeps the records of its calls
//! whatever the switch states.
//!
//! A call that runs a body, such as `eventually`, takes its number before
//! the calls of its body. Each run of the body records into calls of its
//! own, and the call that ran it moves them to its own calls when the run
//! ends, each with the call as its parent.

use crate::failure::Where;
use crate::json::Json;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::sync::{Arc, LazyLock, Mutex, MutexGuard, OnceLock, PoisonError};

/// The definition version that the library implements, which every call
/// record and every store entry states. It is the version of the definition
/// that the conformance crate vendors. The published crate does not contain
/// that copy, so the specs compare this constant with the file `VERSION` of
/// the copy.
pub(crate) const DEFINITION: &str = "10.1.0";

/// The variable that turns recording on.
pub(crate) const VARIABLE: &str = "DOKIMI_ASSERT_RECORD";

/// The suffix of the artifact's file name.
pub(crate) const ARTIFACT_SUFFIX: &str = ".dokimi-assert.jsonl";

/// What the variable states.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Switch {
    /// The seat of a test writes no call record.
    Off,
    /// The seat of a test writes the call record of every call.
    On,
    /// The variable states a value that the definition does not define, and
    /// every call ends with this fault.
    Fault(String),
}

/// Returns the reading of `value`, the value of [`VARIABLE`].
pub(crate) fn read_switch(value: Option<&OsStr>) -> Switch {
    match value.map(OsStr::to_str) {
        None | Some(Some("" | "0")) => Switch::Off,
        Some(Some("1")) => Switch::On,
        Some(_) => Switch::Fault(format!(
            "{VARIABLE}: {:?} is neither 0 nor 1",
            value.unwrap_or_default().to_string_lossy()
        )),
    }
}

/// Returns the reading of the switch, which the process reads once, so a
/// change of the variable during a run has no effect.
pub(crate) fn switch() -> &'static Switch {
    static READ: OnceLock<Switch> = OnceLock::new();
    READ.get_or_init(|| read_switch(std::env::var_os(VARIABLE).as_deref()))
}

/// How a call ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Verdict {
    Pass,
    Fail,
    Error,
}

impl Verdict {
    fn name(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Fail => "fail",
            Self::Error => "error",
        }
    }
}

/// The part of a property's run that called the body: the kind of its case,
/// which a call record states in `phase`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// A case whose values the caller states.
    Example,
    /// A case that the store keeps.
    Stored,
    /// The case whose every choice is its target.
    Simplest,
    /// A random case before the first coverage check.
    Random,
    /// A prefix case.
    Prefix,
    /// An edge case.
    Edge,
    /// A random case after the first coverage check.
    Coverage,
    /// The replay of the failing case before shrinking.
    Replay,
    /// A candidate that the shrinker runs.
    Shrink,
    /// A filling or a boundary step of the explain phase.
    Explain,
    /// The case of a replay token.
    Token,
    /// The case that the fuzz bridge decodes from a fuzzer's input.
    Fuzz,
}

impl Phase {
    /// Returns the name of the phase, as a call record states it.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Example => "example",
            Self::Stored => "stored",
            Self::Simplest => "simplest",
            Self::Random => "random",
            Self::Prefix => "prefix",
            Self::Edge => "edge",
            Self::Coverage => "coverage",
            Self::Replay => "replay",
            Self::Shrink => "shrink",
            Self::Explain => "explain",
            Self::Token => "token",
            Self::Fuzz => "fuzz",
        }
    }
}

/// The record of one call, before the calls that keep it give it a number.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Call {
    pub(crate) assertion: &'static str,
    pub(crate) contract: String,
    pub(crate) verdict: Verdict,
    pub(crate) aborting: bool,
    pub(crate) location: Option<Where>,
    /// The detail as a JSON object, on a failure and for a property's run.
    pub(crate) detail: Option<Json>,
    /// The text of the fault that ended a call of the verdict `error`.
    pub(crate) error: Option<String>,
}

/// Where a numbered call is among the calls of its test.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Place {
    seq: u64,
    parent: Option<u64>,
    run: Option<u64>,
    phase: Option<Phase>,
}

/// Returns the call record of `call` at `place` as one line of JSON, with
/// the field `test` when `test` names one.
fn encode(call: &Call, place: Place, test: Option<&str>) -> String {
    let mut members: Vec<(String, Json)> = vec![
        ("definition".to_owned(), DEFINITION.into()),
        ("seq".to_owned(), place.seq.into()),
    ];
    if let Some(parent) = place.parent {
        members.push(("parent".to_owned(), parent.into()));
    }
    if let Some(run) = place.run {
        members.push(("run".to_owned(), run.into()));
    }
    if let Some(phase) = place.phase {
        members.push(("phase".to_owned(), phase.name().into()));
    }
    members.push(("assertion".to_owned(), call.assertion.into()));
    members.push(("contract".to_owned(), call.contract.as_str().into()));
    members.push(("verdict".to_owned(), call.verdict.name().into()));
    members.push(("aborting".to_owned(), call.aborting.into()));
    if let Some(at) = call.location {
        members.push((
            "where".to_owned(),
            Json::Object(vec![
                ("file".to_owned(), at.base_name().into()),
                ("line".to_owned(), at.line().into()),
            ]),
        ));
    }
    if let Some(detail) = &call.detail {
        members.push(("detail".to_owned(), detail.clone()));
    }
    if let Some(error) = &call.error {
        members.push(("error".to_owned(), error.as_str().into()));
    }
    if let Some(test) = test {
        members.push(("test".to_owned(), test.into()));
    }
    Json::Object(members).to_string()
}

/// The lock that every change of recording state takes first, so that no
/// two changes interleave and no two locks are taken in two orders.
static ORDER: Mutex<()> = Mutex::new(());

fn ordered() -> MutexGuard<'static, ()> {
    ORDER.lock().unwrap_or_else(PoisonError::into_inner)
}

fn locked<T>(lock: &Mutex<T>) -> MutexGuard<'_, T> {
    lock.lock().unwrap_or_else(PoisonError::into_inner)
}

/// How many steps the walk of a run's case has made.
pub(crate) type Position = Arc<dyn Fn() -> usize + Send + Sync>;

/// What a set of calls does with a record.
enum Mode {
    /// A recorder's calls: each record is numbered from 1 and kept.
    Keeping,
    /// A test's calls: each record is numbered within its test and written
    /// to the artifact. The test is the one named, or the test of the
    /// thread that makes each call.
    Writing(Option<String>),
    /// One run of a body: the records stay unnumbered until the call that
    /// ran the body takes them.
    Running(Option<Position>),
}

/// The calls of one seat.
pub struct Calls {
    state: Mutex<State>,
}

struct State {
    mode: Mode,
    next: u64,
    kept: Vec<Option<String>>,
    entries: Vec<Arc<Mutex<Entry>>>,
}

impl std::fmt::Debug for Calls {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Calls").finish_non_exhaustive()
    }
}

/// One call among the calls of its test.
#[derive(Default)]
struct Entry {
    call: Option<Call>,
    parent: Option<Arc<Mutex<Entry>>>,
    run: u64,
    phase: Option<Phase>,
    steps: usize,
    seq: u64,
    home: Option<Arc<Calls>>,
    /// The test that the record names, for a record that goes to the
    /// artifact.
    test: Option<String>,
}

impl Calls {
    fn with(mode: Mode) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(State {
                mode,
                next: 0,
                kept: Vec::new(),
                entries: Vec::new(),
            }),
        })
    }

    /// Returns the calls of a recorder, which keep every record.
    pub(crate) fn keeping() -> Arc<Self> {
        Self::with(Mode::Keeping)
    }

    /// Returns the calls of a seat of the test `test`, which write each
    /// record to the artifact. A seat that names no test writes under the
    /// test of the thread that makes each call.
    pub(crate) fn writing(test: Option<String>) -> Arc<Self> {
        Self::with(Mode::Writing(test))
    }

    /// Returns the calls of one run of a body. `position` reports the steps
    /// of the walk of a property's case, for a run whose calls [`Calls::cut`]
    /// drops.
    pub(crate) fn running(position: Option<Position>) -> Arc<Self> {
        Self::with(Mode::Running(position))
    }

    /// Adds the record of a call that ran no body.
    pub(crate) fn add(self: &Arc<Self>, call: Call) {
        let _order = ordered();
        let entry = Arc::new(Mutex::new(Entry {
            call: Some(call),
            ..Entry::default()
        }));
        self.put(&entry);
    }

    /// Puts `entry` among the calls: numbered and emitted when they keep or
    /// write, and kept unnumbered in a run.
    fn put(self: &Arc<Self>, entry: &Arc<Mutex<Entry>>) {
        let mut state = locked(&self.state);
        locked(entry).home = Some(Arc::clone(self));
        match &state.mode {
            Mode::Running(position) => {
                let steps = position.as_ref().map_or(0, |position| position());
                locked(entry).steps = steps;
                state.entries.push(Arc::clone(entry));
                return;
            }
            Mode::Keeping => {
                state.next += 1;
                let seq = state.next;
                locked(entry).seq = seq;
            }
            Mode::Writing(test) => {
                let test = test.clone().unwrap_or_else(thread_test);
                let seq = number(&test);
                let mut held = locked(entry);
                held.seq = seq;
                held.test = Some(test);
            }
        }
        let ready = locked(entry).call.is_some();
        if ready {
            Self::emit(&mut state, entry);
        }
    }

    /// Writes the record of `entry`, a numbered entry whose call has ended.
    fn emit(state: &mut State, entry: &Arc<Mutex<Entry>>) {
        let held = locked(entry);
        let Some(call) = &held.call else { return };
        let parent = held.parent.as_ref().map(|parent| locked(parent).seq);
        let place = Place {
            seq: held.seq,
            parent,
            run: parent.map(|_| held.run),
            phase: parent.and(held.phase),
        };
        match &state.mode {
            Mode::Writing(_) => {
                let line = encode(call, place, held.test.as_deref());
                artifact::write(&line);
            }
            Mode::Keeping => {
                let line = encode(call, place, None);
                let at = usize::try_from(held.seq - 1).unwrap_or(usize::MAX);
                if state.kept.len() <= at {
                    state.kept.resize(at + 1, None);
                }
                state.kept[at] = Some(line);
            }
            // A run keeps entries and emits none.
            Mode::Running(_) => {}
        }
    }

    /// Returns the records that a recorder's calls keep, one line of JSON
    /// each, in the order of their numbers. A call that still runs its body
    /// has no line yet.
    pub(crate) fn lines(&self) -> Vec<String> {
        let _order = ordered();
        locked(&self.state).kept.iter().flatten().cloned().collect()
    }

    /// Drops the records of the calls that the run made at the step `steps`
    /// of its case's walk and after it: the calls that a run on one worker
    /// does not make, because the case stops at that step there.
    pub(crate) fn cut(&self, steps: usize) {
        let _order = ordered();
        locked(&self.state)
            .entries
            .retain(|entry| locked(entry).steps < steps);
    }
}

/// Returns the name of the test that the current thread runs: libtest names
/// each test's thread after the test's path within its crate.
fn thread_test() -> String {
    std::thread::current().name().unwrap_or_default().to_owned()
}

/// The last number that each test gave a call.
static NUMBERS: LazyLock<Mutex<HashMap<String, u64>>> = LazyLock::new(Mutex::default);

/// Returns the next number of a call of `test`, from 1.
fn number(test: &str) -> u64 {
    let mut numbers = locked(&NUMBERS);
    let next = numbers.entry(test.to_owned()).or_insert(0);
    *next += 1;
    *next
}

/// A call that runs a body: it took its number before the calls of its body,
/// and writes its record when it ends.
pub(crate) struct Slot {
    entry: Arc<Mutex<Entry>>,
    runs: Mutex<u64>,
}

impl Slot {
    /// Starts the record of a call among `calls`: the call takes its number
    /// now, before the calls of its body.
    pub(crate) fn begin(calls: &Arc<Calls>) -> Self {
        let _order = ordered();
        let entry = Arc::new(Mutex::new(Entry::default()));
        calls.put(&entry);
        Self {
            entry,
            runs: Mutex::new(0),
        }
    }

    /// Takes the next run of the slot's body, whose calls `body` keeps, and
    /// moves them to the calls that keep the slot: each with the slot's call
    /// as its parent, the run's number and its phase. A call of a body inside
    /// the run keeps its own parent. `body` is empty afterwards.
    pub(crate) fn take(&self, body: &Calls, phase: Option<Phase>) {
        let _order = ordered();
        let entries = std::mem::take(&mut locked(&body.state).entries);
        let run = {
            let mut runs = locked(&self.runs);
            *runs += 1;
            *runs
        };
        let home = locked(&self.entry)
            .home
            .clone()
            .expect("a slot is among the calls that it began among, or that took it since");
        for entry in entries {
            {
                let mut held = locked(&entry);
                if held.parent.is_none() {
                    held.parent = Some(Arc::clone(&self.entry));
                    held.run = run;
                    held.phase = phase;
                }
            }
            home.put(&entry);
        }
    }

    /// Ends the slot's call with its record, which the calls that keep the
    /// slot write under the number that the call took when it began.
    pub(crate) fn write(&self, call: Call) {
        let _order = ordered();
        let home = {
            let mut held = locked(&self.entry);
            held.call = Some(call);
            held.home
                .clone()
                .expect("a slot is among the calls that it began among, or that took it since")
        };
        Calls::emit(&mut locked(&home.state), &self.entry);
    }
}

/// The artifact of the test binary.
pub(crate) mod artifact {
    use super::{ARTIFACT_SUFFIX, Mutex, PoisonError};
    use std::fs::File;
    use std::io::Write as _;
    use std::path::PathBuf;

    /// The state of the artifact's file in this process.
    enum Artifact {
        /// No record has been written yet.
        Unopened,
        /// The file that the first record created anew.
        Open(File),
        /// The file could not be created, which the standard error states.
        Refused,
    }

    static FILE: Mutex<Artifact> = Mutex::new(Artifact::Unopened);

    /// Returns the path of the artifact: the path of the test binary with the
    /// suffix of the artifact.
    pub(crate) fn path() -> Option<PathBuf> {
        let binary = std::env::current_exe().ok()?;
        let mut name = binary.file_name()?.to_os_string();
        name.push(ARTIFACT_SUFFIX);
        Some(binary.with_file_name(name))
    }

    /// Writes one line to the artifact. A run whose artifact cannot be
    /// created states it once on the standard error and keeps running.
    pub(crate) fn write(line: &str) {
        let mut file = FILE.lock().unwrap_or_else(PoisonError::into_inner);
        if let Artifact::Unopened = *file {
            *file = if let Some(Ok(created)) = path().map(File::create) {
                Artifact::Open(created)
            } else {
                eprintln!("dokimi-assert: the artifact of the call records cannot be created");
                Artifact::Refused
            };
        }
        if let Artifact::Open(open) = &mut *file {
            let _ = writeln!(open, "{line}");
        }
    }
}
