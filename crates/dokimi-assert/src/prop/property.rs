//! The property run that `for_all`, `fuzz` and every property form use.
//!
//! A property takes its settings from its options and from the environment.
//! It replays the failures of its store, and it writes each new failure
//! there. It reports one record of its assertion, `prop-for-all` or the id of
//! a form, at the call site of the property. The record of each call in a
//! case has the phase of the case, and the call of the property as its
//! parent.

use super::case::{Case, Context};
use super::detail::{FOR_ALL, detail_json, record_of};
use super::directory::{self, Loaded, StoreFault};
use super::engine::campaign;
use super::engine::case::{Step, Stop};
use super::engine::choice::Choice;
use super::engine::execution::{Execution, Fault, Phase};
use super::engine::runner::{self, Example, Kind, Outcome, Run as EngineRun, Settings};
use super::engine::store::{self, MAX_DEPTH};
use super::engine::token;
use super::engine::trace::{Entry, Refusal, TraceError};
use super::environment;
use super::options::{Options, Wrapper};
use super::scope::{self, Generators};
use crate::json::{self, Json};
use crate::matcher::panics::{message, quietly};
use crate::record::DEFINITION;
use crate::seat::{Seat, Standard};
use crate::value::Value;
use crate::verdict::{self, Mode, Run};
use std::panic::resume_unwind;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// The call of a property: its operation, which each fault's text starts
/// with, its assertion and its contract.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Call<'c> {
    pub(crate) op: &'static str,
    pub(crate) assertion: &'static str,
    pub(crate) contract: &'c str,
    /// Whether every record of a case is at the call site of the property:
    /// a form calls its assertion from inside the library.
    pub(crate) pinned: bool,
}

/// A fault that ends the call of a property: where it is and what is wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Refused {
    pub(crate) at: String,
    pub(crate) reason: String,
}

impl Refused {
    pub(crate) fn new(at: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            at: at.into(),
            reason: reason.into(),
        }
    }

    /// Returns the text of the fault of an operation.
    fn text(&self, op: &str) -> String {
        if self.at.is_empty() {
            format!("{op}: {}", self.reason)
        } else {
            format!("{op}: {}: {}", self.at, self.reason)
        }
    }
}

impl From<environment::Refused> for Refused {
    fn from(refused: environment::Refused) -> Self {
        Self::new(refused.at, refused.reason)
    }
}

impl From<StoreFault> for Refused {
    fn from(fault: StoreFault) -> Self {
        Self::new(fault.at, fault.reason)
    }
}

/// The name of the draws option, which the location of each fault in its
/// entries starts with.
const DRAWS: &str = "draws";

/// Returns the entries of the draws option's text: a JSON array of draw
/// entries and step entries.
fn entries_of(text: &str) -> Result<Vec<Entry>, Refused> {
    let refused = || Refused::new(DRAWS, "the entries are not a JSON array of objects");
    let parsed = json::parse(text, MAX_DEPTH).map_err(|_| refused())?;
    let items = parsed.as_array().ok_or_else(refused)?;
    let mut entries = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        let at = format!("{DRAWS}[{index}]");
        if item.as_object().is_none() {
            return Err(refused());
        }
        if let Some(action) = item.get("step").and_then(Json::as_str) {
            let client = match item.get("client") {
                None | Some(Json::Null) => None,
                Some(Json::Int(client)) if *client >= 0 => usize::try_from(*client).ok(),
                Some(other) => {
                    return Err(Refused::new(
                        format!("{at}.client"),
                        format!("the client {other} is not an int of 0 or more"),
                    ));
                }
            };
            entries.push(Entry::Step(Step {
                action: action.to_owned(),
                client,
                drain: item.get("drain") == Some(&Json::Bool(true)),
            }));
            continue;
        }
        let (Some(label), Some(value)) =
            (item.get("label").and_then(Json::as_str), item.get("value"))
        else {
            return Err(Refused::new(
                at,
                "the entry has neither a step nor a label and a value",
            ));
        };
        let value = Value::from_literal(value).map_err(|refused| {
            Refused::new(
                format!("{at}.value"),
                format!("the value is not a typed literal: {refused}"),
            )
        })?;
        entries.push(Entry::Draw {
            label: label.to_owned(),
            value,
        });
    }
    Ok(entries)
}

/// Returns the fault at an entry of the draws option that the body cannot
/// follow.
fn refusal(entries: &[Entry], error: &TraceError) -> Refused {
    let at = format!("{DRAWS}[{}].{}", error.entry, error.refusal.name());
    let label = format!("{:?}", error.name);
    match error.refusal {
        Refusal::Step => Refused::new(
            at,
            format!("the machine cannot take the step {label} there"),
        ),
        Refusal::Value => Refused::new(
            at,
            format!(
                "the generator of the draw labelled {label} does not produce the value of the entry"
            ),
        ),
        // A draw refuses the label of an entry that the trace states.
        Refusal::Label => {
            let taken = match &entries[error.entry] {
                Entry::Step(step) => format!("the step entry of {:?}", step.action),
                Entry::Draw { label, .. } => format!("the entry labelled {label:?}"),
            };
            Refused::new(at, format!("the draw labelled {label} takes {taken}"))
        }
    }
}

/// Returns the UTC date of today, as `YYYY-MM-DD`.
fn today() -> String {
    let days = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs() / 86_400);
    let days = i64::try_from(days).unwrap_or_default();
    // The civil date of a count of days since 1970-01-01, after Howard
    // Hinnant's days_from_civil algorithm run backwards.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

/// A property with its settings and its store.
pub(crate) struct Property<'c> {
    pub(crate) call: Call<'c>,
    seed: u64,
    cases: usize,
    max_choices: usize,
    requirements: Vec<runner::Requirement>,
    shrink: usize,
    shrink_time: Option<Duration>,
    explain: bool,
    workers: usize,
    entries: Option<Vec<Entry>>,
    /// The length of a campaign in whole seconds, and 0 for a property that
    /// runs no campaign.
    budget: u64,
    replay: Option<Vec<Choice>>,
    directory: Option<PathBuf>,
    around: Option<Wrapper>,
    /// The generators that `using` gives types in the runs, or `None` for
    /// runs that state none.
    using: Option<Arc<Generators>>,
    test: Option<String>,
}

impl<'c> Property<'c> {
    /// Returns the property of a call on `seat` under `options`.
    pub(crate) fn of<T>(
        seat: &dyn Seat,
        call: Call<'c>,
        options: Options<T>,
    ) -> Result<Self, Refused> {
        let hermetic = options.hermetic;
        let seed = environment::seed(call.contract, options.seed, hermetic)?;
        let budget = environment::budget(hermetic)?;
        let entries = options.draws.as_deref().map(entries_of).transpose()?;
        let replay = match environment::token(options.replay.as_deref(), hermetic) {
            None => None,
            Some((text, source)) => Some(token::decode(&text).map_err(|refused| {
                Refused::new(source, format!("the text is not a replay token: {refused}"))
            })?),
        };
        let test = seat.test();
        Ok(Self {
            call,
            seed,
            cases: options.cases,
            max_choices: options.max_choices,
            requirements: options.requirements,
            shrink: options.shrink,
            shrink_time: Some(options.shrink_time).filter(|limit| !limit.is_zero()),
            explain: options.explain,
            workers: options.workers,
            entries,
            budget,
            replay,
            directory: directory::store_of(test.as_deref(), options.store.as_deref()),
            around: options.around,
            using: options.using.generators(),
            test,
        })
    }

    /// Returns the settings of a run of the property.
    fn settings(&self, examples: Vec<Example>, stored: Vec<Vec<Choice>>) -> Settings {
        let mut settings = Settings::new(self.seed);
        settings.cases = self.cases;
        settings.max_choices = self.max_choices;
        settings.requirements.clone_from(&self.requirements);
        settings.traces = self.entries.iter().cloned().collect();
        settings.examples = examples;
        settings.stored = stored;
        settings.shrink = self.shrink;
        settings.shrink_time = self.shrink_time;
        settings.explain = self.explain;
        settings.workers = self.workers;
        settings
    }

    /// Ends the call with a fault of the property's operation.
    #[track_caller]
    pub(crate) fn fault(&self, run: &Run<'_>, refused: &Refused) {
        fault(run, self.call, refused);
    }

    /// Runs `body` as the property on `seat`, and reports the outcome.
    ///
    /// A property that replays a token runs only the case of the token. Any
    /// other property replays its stored cases first. Under the campaign
    /// profile, it then runs a campaign for its budget, which writes an entry
    /// of each failure to the store as it concludes the failure. Otherwise it
    /// runs its cases and writes an entry of each failure of a counterexample
    /// to the store.
    #[track_caller]
    pub(crate) fn run(
        &self,
        seat: &dyn Seat,
        run: &Run<'_>,
        body: &(dyn Fn(&Case<'_>) + Sync),
        examples: Vec<Example>,
    ) {
        if !self.claim(run) {
            return;
        }
        let context = self.context(seat, run);
        if let Some(choices) = &self.replay {
            let mut settings = self.settings(examples, Vec::new());
            settings.replay = Some(choices.clone());
            if let Some(outcome) =
                self.drive(run, &context, body, |engine| runner::run(engine, settings))
            {
                self.report(run, &outcome);
            }
            return;
        }
        let loaded = match self.load() {
            Ok(loaded) => loaded,
            Err(refused) => return self.fault(run, &refused),
        };
        let stored = loaded
            .entries
            .iter()
            .map(|entry| entry.choices.clone())
            .collect();
        let settings = self.settings(examples, stored);
        let mut saved = Vec::new();
        let mut keep = |found: &Outcome| saved.extend(self.save(found));
        let explored = self.drive(run, &context, body, |engine| match self.budget {
            0 => runner::run(engine, settings),
            seconds => {
                campaign::campaign(engine, settings, Duration::from_secs(seconds), &mut keep)
            }
        });
        let Some(outcome) = explored else {
            return;
        };
        let mut notes = self.store_notes(&loaded, &outcome);
        if outcome.kind == Kind::Counterexample && self.budget == 0 {
            notes.extend(self.save(&outcome));
        }
        notes.append(&mut saved);
        self.note(seat, &notes);
        self.report(run, &outcome);
    }

    /// Replays the stored cases of the property, and reports a pass that
    /// counts them, or the first stored case that does not pass, as found.
    #[track_caller]
    pub(crate) fn replay_stored(
        &self,
        seat: &dyn Seat,
        run: &Run<'_>,
        body: &(dyn Fn(&Case<'_>) + Sync),
    ) {
        if !self.claim(run) {
            return;
        }
        let loaded = match self.load() {
            Ok(loaded) => loaded,
            Err(refused) => return self.fault(run, &refused),
        };
        let context = self.context(seat, run);
        let stored = loaded
            .entries
            .iter()
            .map(|entry| entry.choices.clone())
            .collect();
        let settings = self.settings(Vec::new(), stored);
        let Some(outcome) = self.drive(run, &context, body, |engine| {
            runner::run_stored(engine, settings)
        }) else {
            return;
        };
        self.note(seat, &self.store_notes(&loaded, &outcome));
        self.report(run, &outcome);
    }

    /// Runs the case that the fuzz bridge decodes from `data`, and reports
    /// it: a pass counts one valid or one rejected case, and a failing case is
    /// replayed, shrunk, explained and stored as a failing case of a run is.
    #[track_caller]
    pub(crate) fn input(
        &self,
        seat: &dyn Seat,
        run: &Run<'_>,
        body: &(dyn Fn(&Case<'_>) + Sync),
        data: &[u8],
    ) {
        let context = self.context(seat, run);
        let settings = self.settings(Vec::new(), Vec::new());
        let bridged = |engine: &mut EngineRun<'_, '_>| -> Result<Outcome, Fault> {
            let provider = Box::new(super::engine::bridge::Bridging::new(data));
            let execution = engine.execute(provider, settings.max_choices, None)?;
            (engine.observer)(Phase::Fuzz, &execution);
            let mut tally = runner::Tally::new(settings.seed);
            match tally.take(execution) {
                None => Ok(tally.outcome(Kind::Passed, None)),
                Some(found) => runner::conclude(engine, &settings, found),
            }
        };
        let Some(outcome) = self.drive(run, &context, body, bridged) else {
            return;
        };
        if outcome.kind == Kind::Counterexample {
            self.note(seat, &self.save(&outcome));
        }
        self.report(run, &outcome);
    }

    /// Returns the context of the cases of a run of the property on `seat`.
    fn context<'s>(&self, seat: &'s dyn Seat, run: &Run<'_>) -> Context<'s> {
        Context {
            parent: seat,
            site: Some(run.location()),
            pinned: self.call.pinned,
            recorded: run.recorded(),
        }
    }

    /// Claims the entries of the property in its store, and ends the call with
    /// a fault when another property of the test has claimed them.
    #[track_caller]
    fn claim(&self, run: &Run<'_>) -> bool {
        let Some(directory) = &self.directory else {
            return true;
        };
        if directory::claim(directory, self.call.contract) {
            return true;
        }
        self.fault(
            run,
            &Refused::new(
                directory.display().to_string(),
                format!(
                    "two properties of the test have the contract {:?}, and they would share their stored cases",
                    self.call.contract
                ),
            ),
        );
        false
    }

    /// Returns the entries of the store.
    fn load(&self) -> Result<Loaded, Refused> {
        match &self.directory {
            None => Ok(Loaded::default()),
            Some(directory) => Ok(directory::load(directory, self.call.contract)?),
        }
    }

    /// Runs `work` with the property's body, and returns its outcome, or
    /// `None` after the call ended with a fault: an entry of the draws option
    /// that the body cannot follow, or a fault of a case.
    ///
    /// The generators that `using` states apply on the thread of the call and
    /// in each run of the body, on any thread.
    #[track_caller]
    fn drive<'s>(
        &self,
        run: &Run<'_>,
        context: &'s Context<'s>,
        body: &(dyn Fn(&Case<'s>) + Sync),
        work: impl FnOnce(&mut EngineRun<'_, 's>) -> Result<Outcome, Fault>,
    ) -> Option<Outcome> {
        let _run = scope::enter(self.using.clone());
        let around = self.around.clone();
        let using = self.using.clone();
        let adapted = move |case: &Case<'s>| {
            let _run = scope::enter(using.clone());
            match &around {
                None => body(case),
                Some(wrapper) => wrapped(wrapper, case, body),
            }
        };
        let mut observer = |phase: Phase, execution: &Execution| {
            for calls in execution.calls.iter().flatten() {
                if let Some(stopped) = execution.stopped {
                    calls.cut(stopped);
                }
                run.take_in_phase(Some(calls), Some(phase));
            }
        };
        let mut engine = EngineRun {
            body: &adapted,
            context: Some(context),
            observer: &mut observer,
        };
        match work(&mut engine) {
            Ok(outcome) => Some(outcome),
            Err(Fault::Trace(error)) => {
                let entries = self.entries.as_deref().unwrap_or_default();
                self.fault(run, &refusal(entries, &error));
                None
            }
            Err(Fault::Run(text)) => {
                self.fault(run, &Refused::new("", text));
                None
            }
        }
    }

    /// Writes an entry of each failure of a counterexample to the store, and
    /// returns the faults of the entries that the store does not keep. No
    /// entry is written without a store, in a process of a mutation run, or
    /// for a counterexample of values, whose values the test's source states.
    fn save(&self, outcome: &Outcome) -> Vec<Refused> {
        let Some(directory) = &self.directory else {
            return Vec::new();
        };
        if environment::mutated() || outcome.valued {
            return Vec::new();
        }
        let found = today();
        let mut faults = Vec::new();
        for execution in outcome.failing.iter().chain(&outcome.others) {
            let failed = execution
                .failure
                .as_ref()
                .expect("a failed case has its failure");
            let failure = store::Failure {
                contract: self.call.contract.to_owned(),
                choices: execution.record.choices.clone(),
                identity: failed
                    .entry
                    .iter()
                    .map(|(key, value)| ((*key).to_owned(), value.clone()))
                    .collect(),
                counterexample: execution
                    .record
                    .draws
                    .iter()
                    .map(|drawn| (drawn.label.clone(), drawn.value.literal.clone()))
                    .collect(),
            };
            let name = store::name(self.call.contract, &failure.choices);
            let entry = store::entry(&failure, DEFINITION, &found);
            if let Err(error) = directory::save(directory, &name, &entry) {
                faults.push(Refused::new(
                    directory.display().to_string(),
                    format!(
                        "the store does not keep a case of {:?}: {error}",
                        self.call.contract
                    ),
                ));
            }
        }
        faults
    }

    /// Returns the faults of the store that the run notes: each file that it
    /// skipped, and each stored case that decodes to other values than its
    /// entry records.
    fn store_notes(&self, loaded: &Loaded, outcome: &Outcome) -> Vec<Refused> {
        let Some(directory) = &self.directory else {
            return Vec::new();
        };
        let mut notes: Vec<Refused> = loaded
            .skipped
            .iter()
            .map(|name| {
                Refused::new(
                    directory.join(name).display().to_string(),
                    "the entry is of a later format, so the run skips it",
                )
            })
            .collect();
        for (draws, entry) in outcome.stored.iter().zip(&loaded.entries) {
            if directory::differs(&entry.counterexample, draws) {
                notes.push(Refused::new(
                    directory.join(&entry.name).display().to_string(),
                    "the stored case decodes to other values than it records, and the run tested those",
                ));
            }
        }
        notes
    }

    /// Notes each fault that does not end the call through the seat's
    /// `report_fault`. A seat that keeps no fault prints it to the standard
    /// error, which the test runner shows with the output of the test.
    fn note(&self, seat: &dyn Seat, faults: &[Refused]) {
        for fault in faults {
            verdict::note(seat, fault.text(self.call.op));
        }
    }

    /// Reports the outcome as the verdict of the call.
    #[track_caller]
    fn report(&self, run: &Run<'_>, outcome: &Outcome) {
        if outcome.kind == Kind::Passed {
            run.pass_run(
                Mode::Fatal,
                self.call.assertion,
                self.call.contract,
                detail_json(outcome),
            );
            return;
        }
        let (failure, detail) = record_of(
            outcome,
            self.call.assertion,
            self.call.contract,
            Some(run.location()),
        );
        run.fail_run(Mode::Fatal, &failure, detail);
    }

    /// Returns the path of the test that the property belongs to.
    pub(crate) fn test(&self) -> Option<&str> {
        self.test.as_deref()
    }
}

/// Ends the call of a property with a fault of its operation.
#[track_caller]
pub(crate) fn fault(run: &Run<'_>, call: Call<'_>, refused: &Refused) {
    run.fault(
        Mode::Fatal,
        call.assertion,
        call.contract,
        refused.text(call.op),
    );
}

/// Runs a case inside a wrapper of the around option, and ends the case as
/// its body ended. A wrapper that does not call the run, calls it a second
/// time or panics ends the call with a fault.
fn wrapped<'s>(wrapper: &Wrapper, case: &Case<'s>, body: &(dyn Fn(&Case<'s>) + Sync)) {
    let mut calls = 0;
    let mut ending = None;
    let mut once = || {
        calls += 1;
        if calls == 1 {
            ending = Some(quietly(|| body(case)));
        }
    };
    let raised = quietly(|| wrapper(&mut once));
    if let Err(payload) = raised {
        case.end(Stop::Fault(format!(
            "around: the wrapper panicked: {}",
            message(&*payload)
        )));
    }
    match (calls, ending) {
        (0, _) => case.end(Stop::Fault(
            "around: the wrapper returned without calling run".to_owned(),
        )),
        (1, Some(Err(payload))) => resume_unwind(payload),
        (1, _) => {}
        _ => case.end(Stop::Fault(
            "around: the wrapper called run a second time".to_owned(),
        )),
    }
}

/// Fails when `body` fails for a case that a run generates.
///
/// A run tries the case of [`Options::draws`] first. It then replays the
/// stored cases of the property, oldest first, and runs the case whose every
/// choice is its target. Then it runs random cases of the seed, each followed
/// by a prefix case and an edge case. The run stops when it has run the
/// stated number of valid cases, when it has tested every input of the
/// domain, or when it has generated ten times that number of cases. A failing
/// case is replayed, shrunk to the smallest case that fails the same way, and
/// explained.
///
/// A run without a failing case still fails when it rejected more than ten
/// cases for every valid one, when no case requested an input, or when it
/// refuted a coverage requirement or left one unmet.
///
/// The campaign profile, which `DOKIMI_ASSERT_PROP_PROFILE=campaign` selects,
/// runs a campaign instead of a run. A campaign tries the case of the draws
/// option and the stored cases. It then explores for as long as
/// `DOKIMI_ASSERT_PROP_BUDGET` states in whole seconds of the platform clock.
/// A valid case that counts a new label, records a new fingerprint or records
/// a better score with [`Case::target`] joins the campaign's pool, and three
/// cases in four mutate a member of the pool. The campaign shrinks, explains
/// and stores each failure of a new identity when it finds the failure. Its
/// record states the first failure with every later one among its others.
/// Without a failure, the record states the outcome that the last check of a
/// run decides over every valid case. A hermetic run and a run in a process
/// of a mutation run run no campaign.
///
/// The failure is one record of `prop-for-all`, whose detail has the ten
/// fields of the definition, and whose sentence lists the counterexample and
/// the replay token. The call ends with a fault before a run when a
/// `DOKIMI_ASSERT_PROP_*` variable states no value of its kind, the replay
/// token is no token, the draws state no entries, the store cannot be read or
/// has a damaged file, or a second property of the test has the same
/// contract and store. A failure is written to the store of the test, under
/// `testdata/prop/<target>/<test path>/` in the package.
///
/// ```should_panic
/// use dokimi_assert::{Collector, check, prop};
///
/// let t = Collector::new();
/// prop::for_all(&t, "every square is even", prop::hermetic(), |c| {
///     let n = c.draw(&prop::integer(0..=100_u32), "n");
///     check::is_true(c, n * n % 2 == 0, "the square is even");
/// });
/// ```
#[track_caller]
pub fn for_all(
    seat: &dyn Seat,
    contract: &str,
    options: Options,
    body: impl Fn(&Case<'_>) + Send + Sync,
) {
    seat.helper();
    let run = Run::begin(seat);
    let call = Call {
        op: "prop::for_all",
        assertion: FOR_ALL,
        contract,
        pinned: false,
    };
    match Property::of(seat, call, options) {
        Ok(property) => property.run(seat, &run, &body, Vec::new()),
        Err(refused) => fault(&run, call, &refused),
    }
}

/// Replays the stored cases and the seed files of a property, and returns
/// the target of a fuzzer, which runs the case of one input.
///
/// `fuzz` first replays the stored cases of the property, oldest first, as
/// [`for_all`] does, and reports the first stored case that fails as found.
/// It runs no random case under any profile, because `for_all` checks the
/// property with the same body and contract. Then each seed file of the test,
/// under `testdata/fuzz/<target>/<test path>/`, runs as one input in the order
/// of the file names.
///
/// The target decodes each input into the choices of one case by the bridge
/// rules of the definition, so every input is a valid case, and runs it as a
/// call of its own on a [`Standard`] seat. A failing input is replayed,
/// shrunk, explained and stored, and the target then panics with the sentence
/// of the record, which contains the replay token. A target of `cargo fuzz`
/// calls the returned function from `fuzz_target!`.
#[track_caller]
pub fn fuzz<'a>(
    seat: &'a dyn Seat,
    contract: &'a str,
    options: Options,
    body: impl Fn(&Case<'_>) + Send + Sync + 'a,
) -> impl Fn(&[u8]) + 'a {
    seat.helper();
    let run = Run::begin(seat);
    let call = Call {
        op: "prop::fuzz",
        assertion: FOR_ALL,
        contract,
        pinned: false,
    };
    let site = run.location();
    let prepared = Property::of(seat, call, options).and_then(|property| {
        let inputs = match directory::seeds_of(property.test()) {
            Some(directory) => directory::seeds(&directory)?,
            None => Vec::new(),
        };
        Ok((property, inputs))
    });
    let property = match prepared {
        Ok((property, inputs)) => {
            property.replay_stored(seat, &run, &body);
            for (_, data) in &inputs {
                let input = Run::begin_at(seat, site);
                property.input(seat, &input, &body, data);
            }
            Some(property)
        }
        Err(refused) => {
            fault(&run, call, &refused);
            None
        }
    };
    move |data: &[u8]| {
        let standard = Standard::new();
        let input = Run::begin_at(&standard, site);
        match &property {
            Some(property) => property.input(&standard, &input, &body, data),
            None => input.fault(
                Mode::Fatal,
                FOR_ALL,
                contract,
                format!("{}: the property ended with a fault", call.op),
            ),
        }
    }
}
