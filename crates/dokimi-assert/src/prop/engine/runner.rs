// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The phases, the counts and the outcome of a run.
//!
//! The runner calls a body once per case, in four phases, and stops at the
//! first failing case:
//!
//! 1. The known cases: each trace of a case whose draws and steps the caller
//!    states, then each example of a case whose values the caller states,
//!    then each stored choice sequence, oldest first. These cases do not enter
//!    the case tree. A trace that the body cannot follow ends the run before
//!    any other case runs.
//! 2. The simplest case, whose every choice is its target.
//! 3. The random cases: case `i` of the seed, for `i = 0, 1, 2, ...`, until
//!    `cases` valid cases have run, the domain is exhausted, or ten times
//!    `cases` random cases have run, repeats included. A prefix case follows
//!    each random case while at most `min(cases / 10, 50)` cases are valid and
//!    the random case made at least two choices. The next edge case follows
//!    too, until all four have run, and the edge cases that are left when the
//!    random cases stop run then.
//! 4. The coverage cases: when a requirement is undecided at a check, further
//!    random cases up to the next check, at 2, 4 and 8 times `cases`.
//!
//! A valid case passed and repeats no earlier case. A failing case is
//! replayed, shrunk and explained, and the run ends as a counterexample, or as
//! flaky when the replay differs. A run that found no failing case fails
//! anyway when it rejected more than ten times as many cases as were valid,
//! when no case requested an input, or when a coverage requirement is refuted
//! or unmet, checked in that order.
//!
//! A case that skips before the run finds a failing case ends the run as
//! skipped, in every phase. Once the run has found a failing case, a case
//! that skips counts as a case that does not fail: a replay that skips makes
//! the run flaky, a shrink candidate that skips is not accepted, and an
//! explain run that skips neither fails the way the failure fails nor passes.

use super::ahead::Executor;
use super::case::{Drawn, Generating, MAX_CHOICES, Provider, Replaying};
use super::choice::Choice;
use super::coverage::{self, CHECKS, Verdict};
use super::edge::{BOUNDARIES, Edge};
use super::execution::{Body, Divergence, Execution, Fault, Phase, Status, execute};
use super::shrink::{DEFAULT_BUDGET, Explained, Shrinker, confirm, explain};
use super::source::case_source;
use super::token;
use super::trace::{Entry, Tracing};
use super::tree::Tree;
use crate::prop::case::Context;
use std::any::Any;
use std::collections::BTreeMap;
use std::time::Duration;

/// The valid cases that a run aims for when the caller states no number.
pub const DEFAULT_CASES: usize = 100;

/// A run that rejects more than this many cases for every valid one ends as
/// rejected.
const MAX_REJECTIONS: usize = 10;

/// A phase generates at most this many times as many random cases as the
/// valid cases that it aims for.
const GENERATION_FACTOR: usize = 10;

/// A run tries prefix cases while at most `cases / PREFIX_DIVISOR` of its
/// cases are valid, and never past `MAX_PREFIX_CASES` valid cases.
const PREFIX_DIVISOR: usize = 10;
const MAX_PREFIX_CASES: usize = 50;

/// A random case has a prefix case only when it made at least this many
/// choices: one to replay and one to give its target.
const MIN_PREFIXED_CHOICES: usize = 2;

/// How a run ended. `Passed` passes the test, `Skipped` skips it where the
/// test runner can skip a running test, and every other kind fails it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// No case failed, and every requirement is met.
    Passed,
    /// A case failed.
    Counterexample,
    /// A replay of a failing case differed, or the body diverged.
    Flaky,
    /// The run rejected too many cases.
    Rejected,
    /// A coverage requirement is refuted or unmet.
    CoverageUnmet,
    /// No case requested an input.
    Vacuous,
    /// A case skipped the run before the run found a failing case.
    Skipped,
}

impl Kind {
    /// Returns the name of the kind, as a record states it.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Counterexample => "counterexample",
            Self::Flaky => "flaky",
            Self::Rejected => "rejected",
            Self::CoverageUnmet => "coverage-unmet",
            Self::Vacuous => "vacuous",
            Self::Skipped => "skipped",
        }
    }
}

/// A label that must count at least `share` of the valid cases.
#[derive(Debug, Clone, PartialEq)]
pub struct Requirement {
    /// The label.
    pub label: String,
    /// The share, from 0 to 1.
    pub share: f64,
}

/// A case that the caller states: the choices that decode to its values, or
/// the values of its draws, for a generator without an inverse.
pub enum Example {
    /// The choices of the case.
    Choices(Vec<Choice>),
    /// The value of each draw, in order.
    Values(Vec<Box<dyn Any + Send>>),
}

impl std::fmt::Debug for Example {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Choices(choices) => f.debug_tuple("Choices").field(choices).finish(),
            Self::Values(values) => f.debug_tuple("Values").field(&values.len()).finish(),
        }
    }
}

/// What a run is asked to do.
#[derive(Debug)]
pub struct Settings {
    /// The seed of the run.
    pub seed: u64,
    /// The valid cases that the run aims for.
    pub cases: usize,
    /// The most choices of a case.
    pub max_choices: usize,
    /// The coverage requirements, in order.
    pub requirements: Vec<Requirement>,
    /// The cases whose draws and steps the caller states, which run first.
    pub traces: Vec<Vec<Entry>>,
    /// The cases whose values the caller states, which run next.
    pub examples: Vec<Example>,
    /// The stored choice sequences, oldest first.
    pub stored: Vec<Vec<Choice>>,
    /// The runs that shrinking and explaining every failure may spend. A
    /// budget of 0 turns both off.
    pub shrink: usize,
    /// The time that shrinking and explaining may take on the platform clock.
    pub shrink_time: Option<Duration>,
    /// Whether the run explains a counterexample.
    pub explain: bool,
    /// The cases that run at once on threads.
    pub workers: usize,
    /// The one case that the run tries, for a run of a replay token.
    pub replay: Option<Vec<Choice>>,
}

impl Settings {
    /// Returns the settings of a run of `seed` with every other setting at its
    /// default.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self {
            seed,
            cases: DEFAULT_CASES,
            max_choices: MAX_CHOICES,
            requirements: Vec::new(),
            traces: Vec::new(),
            examples: Vec::new(),
            stored: Vec::new(),
            shrink: DEFAULT_BUDGET,
            shrink_time: None,
            explain: true,
            workers: 1,
            replay: None,
        }
    }
}

/// A requirement that a run refuted or left unmet, with its counts.
#[derive(Debug, Clone, PartialEq)]
pub struct Shortfall {
    /// The requirement.
    pub requirement: Requirement,
    /// The valid cases that counted its label.
    pub counted: usize,
    /// The valid cases.
    pub valid: usize,
    /// The verdict: refuted or unmet.
    pub verdict: Verdict,
}

/// The end of a run.
///
/// `cases` counts the valid cases. `failing` is the minimal failing case of a
/// counterexample, or the case that a flaky replay contradicted, and `token`
/// is its replay token. `others` are the minimal cases of the other failures,
/// in the order that they were found. `runs` counts the runs that shrinking
/// and explaining spent. `valued` is set for a counterexample that an example
/// of values found, which has no choices and so no token. `reason` is the
/// reason of a skipped run.
#[derive(Debug)]
pub struct Outcome {
    /// How the run ended.
    pub kind: Kind,
    /// The valid cases.
    pub cases: usize,
    /// The rejected cases.
    pub rejected: usize,
    /// The seed of the run.
    pub seed: u64,
    /// The failing case.
    pub failing: Option<Execution>,
    /// The difference that made the run flaky.
    pub divergence: Option<Divergence>,
    /// The requirement that a coverage-unmet run missed.
    pub shortfall: Option<Shortfall>,
    /// The minimal cases of the other failures.
    pub others: Vec<Execution>,
    /// The explanation of each draw of the counterexample.
    pub explanation: Vec<Explained>,
    /// The replay token of the failing case.
    pub token: Option<String>,
    /// The runs that shrinking and explaining spent.
    pub runs: usize,
    /// Whether an example of values found the counterexample.
    pub valued: bool,
    /// The reason of the case that skipped a skipped run.
    pub reason: Option<String>,
    /// The draws of each stored case that ran, up to the one that ended the
    /// run.
    pub(crate) stored: Vec<Vec<Drawn>>,
}

/// The counts of a run so far.
#[derive(Debug)]
pub(crate) struct Tally {
    seed: u64,
    pub(crate) valid: usize,
    pub(crate) rejected: usize,
    requested: bool,
    labels: BTreeMap<String, usize>,
}

impl Tally {
    pub(crate) fn new(seed: u64) -> Self {
        Self {
            seed,
            valid: 0,
            rejected: 0,
            requested: false,
            labels: BTreeMap::new(),
        }
    }

    /// Counts one execution, and returns it when it ends the run: a failed
    /// case ends it with a counterexample, a diverged case as flaky, and a
    /// skipped case as skipped with its reason.
    pub(crate) fn take(&mut self, execution: Execution) -> Option<Outcome> {
        let record = &execution.record;
        self.requested |= !record.draws.is_empty() || !record.choices.is_empty();
        match execution.status {
            Status::Failed => return Some(self.outcome(Kind::Counterexample, Some(execution))),
            Status::Diverged => {
                let divergence = execution.divergence.clone();
                let mut outcome = self.outcome(Kind::Flaky, None);
                outcome.divergence = divergence;
                return Some(outcome);
            }
            Status::Skipped => {
                let mut outcome = self.outcome(Kind::Skipped, None);
                outcome.reason = execution.reason;
                return Some(outcome);
            }
            Status::Rejected => self.rejected += 1,
            Status::Passed => {
                self.valid += 1;
                for label in &record.labels {
                    *self.labels.entry(label.clone()).or_default() += 1;
                }
            }
            Status::Repeated => {}
        }
        None
    }

    /// Returns an outcome of `kind` with the counts so far.
    pub(crate) fn outcome(&self, kind: Kind, failing: Option<Execution>) -> Outcome {
        Outcome {
            kind,
            cases: self.valid,
            rejected: self.rejected,
            seed: self.seed,
            failing,
            divergence: None,
            shortfall: None,
            others: Vec::new(),
            explanation: Vec::new(),
            token: None,
            runs: 0,
            valued: false,
            reason: None,
            stored: Vec::new(),
        }
    }

    /// Returns the outcome of too many rejections, or of no input.
    pub(crate) fn failure_without_counterexample(&self) -> Option<Outcome> {
        if self.rejected > MAX_REJECTIONS * self.valid {
            return Some(self.outcome(Kind::Rejected, None));
        }
        if !self.requested {
            return Some(self.outcome(Kind::Vacuous, None));
        }
        None
    }

    /// Returns the first requirement that the counts refute or leave unmet,
    /// and whether they meet every requirement.
    pub(crate) fn shortfall(
        &self,
        requirements: &[Requirement],
        last: bool,
        exact: bool,
    ) -> (Option<Shortfall>, bool) {
        let mut met = true;
        for requirement in requirements {
            let counted = self
                .labels
                .get(&requirement.label)
                .copied()
                .unwrap_or_default();
            let found = coverage::verdict(counted, self.valid, requirement.share, last, exact);
            if matches!(found, Verdict::Refuted | Verdict::Unmet) {
                let shortfall = Shortfall {
                    requirement: requirement.clone(),
                    counted,
                    valid: self.valid,
                    verdict: found,
                };
                return (Some(shortfall), false);
            }
            met &= found == Verdict::Met;
        }
        (None, met)
    }
}

/// What watches each call of a run's body, with its phase, in the order of a
/// run on one worker.
pub(crate) type Observer<'o> = &'o mut dyn FnMut(Phase, &Execution);

/// The run of a body: its settings, its context and its observer.
pub(crate) struct Run<'r, 's> {
    pub(crate) body: Body<'r, 's>,
    pub(crate) context: Option<&'s Context<'s>>,
    pub(crate) observer: Observer<'r>,
}

impl Run<'_, '_> {
    /// Calls the body once outside the case tree.
    pub(crate) fn execute(
        &self,
        provider: Box<dyn Provider>,
        max_choices: usize,
        limit: Option<usize>,
    ) -> Result<Execution, Fault> {
        execute(self.body, provider, max_choices, self.context, limit)
    }
}

/// Returns the most valid cases at which a run tries a prefix case.
fn prefix_cases(cases: usize) -> usize {
    (cases / PREFIX_DIVISOR).min(MAX_PREFIX_CASES)
}

/// Takes the known cases of a run out of `settings`, in order: each trace,
/// each example and each stored case, with its phase and whether it states
/// the values of its draws.
pub(crate) fn known_cases(settings: &mut Settings) -> Vec<(Phase, Box<dyn Provider>, bool)> {
    let mut known: Vec<(Phase, Box<dyn Provider>, bool)> = Vec::new();
    for trace in std::mem::take(&mut settings.traces) {
        known.push((Phase::Example, Box::new(Tracing::new(trace)), false));
    }
    for example in std::mem::take(&mut settings.examples) {
        known.push(match example {
            Example::Choices(choices) => (Phase::Example, Box::new(Replaying::new(choices)), false),
            Example::Values(values) => (
                Phase::Example,
                Box::new(Replaying::stating(Vec::new(), values)),
                true,
            ),
        });
    }
    for choices in &settings.stored {
        known.push((
            Phase::Stored,
            Box::new(Replaying::new(choices.clone())),
            false,
        ));
    }
    known
}

/// Runs the known cases outside the case tree until one ends the run.
/// `stored` receives the draws of each stored case that ran.
fn try_known(
    run: &mut Run<'_, '_>,
    settings: &mut Settings,
    tally: &mut Tally,
    stored: &mut Vec<Vec<Drawn>>,
) -> Result<Option<Outcome>, Fault> {
    for (phase, provider, valued) in known_cases(settings) {
        let execution = run.execute(provider, settings.max_choices, None)?;
        (run.observer)(phase, &execution);
        if phase == Phase::Stored {
            stored.push(execution.record.draws.clone());
        }
        if let Some(mut outcome) = tally.take(execution) {
            outcome.valued = valued;
            return Ok(Some(outcome));
        }
    }
    Ok(None)
}

/// The cases of a run after its known cases, in the order that one worker
/// runs them.
struct Phases<'p, 'e, 'r, 's> {
    run: &'p mut Run<'r, 's>,
    settings: &'p Settings,
    tally: &'p mut Tally,
    executor: &'p mut Executor<'e, 's>,
    tree: Tree,
    edges: usize,
    index: u64,
    phase: Phase,
}

impl Phases<'_, '_, '_, '_> {
    /// Passes one case of `phase` to the observer and counts it.
    fn seen(&mut self, phase: Phase, execution: Execution) -> Option<Outcome> {
        (self.run.observer)(phase, &execution);
        self.tally.take(execution)
    }

    fn simplest(&mut self) -> Result<Option<Outcome>, Fault> {
        let execution = self.executor.replay(&mut self.tree, Vec::new())?;
        Ok(self.seen(Phase::Simplest, execution))
    }

    /// Runs the next random case, then its prefix case and the next edge
    /// case.
    fn random(&mut self) -> Result<Option<Outcome>, Fault> {
        let generated = self.executor.random(&mut self.tree, self.index)?;
        self.index += 1;
        let (execution, choices, mut source) = generated;
        if let Some(outcome) = self.seen(self.phase, execution) {
            return Ok(Some(outcome));
        }
        let prefixed = self.tally.valid <= prefix_cases(self.settings.cases)
            && choices.len() >= MIN_PREFIXED_CHOICES
            && !self.tree.exhausted();
        if prefixed {
            let span = super::draw::wide(choices.len() - 1);
            let cut = 1 + usize::try_from(source.below(span)).unwrap_or_default();
            let execution = self
                .executor
                .replay(&mut self.tree, choices[..cut].to_vec())?;
            if let Some(outcome) = self.seen(Phase::Prefix, execution) {
                return Ok(Some(outcome));
            }
        }
        self.next_edge()
    }

    /// Runs the next edge case, unless none is left or the domain is
    /// exhausted.
    fn next_edge(&mut self) -> Result<Option<Outcome>, Fault> {
        if self.edges == BOUNDARIES.len() || self.tree.exhausted() {
            return Ok(None);
        }
        let execution = self.executor.edge(&mut self.tree, self.edges)?;
        self.edges += 1;
        Ok(self.seen(Phase::Edge, execution))
    }

    /// Runs random cases until `target` cases are valid, then the edge cases
    /// that are left.
    fn run_to(&mut self, target: usize) -> Result<Option<Outcome>, Fault> {
        let limit = u64::try_from(GENERATION_FACTOR * target).unwrap_or(u64::MAX);
        while self.tally.valid < target && !self.tree.exhausted() && self.index < limit {
            if let Some(outcome) = self.random()? {
                return Ok(Some(outcome));
            }
        }
        while self.edges < BOUNDARIES.len() && !self.tree.exhausted() {
            if let Some(outcome) = self.next_edge()? {
                return Ok(Some(outcome));
            }
        }
        Ok(None)
    }

    /// Runs the random phase to each check, and returns the outcome that
    /// ends the run, or `None` when it passes.
    fn checks(&mut self) -> Result<Option<Outcome>, Fault> {
        let requirements = &self.settings.requirements;
        let multiples: &[usize] = if requirements.is_empty() {
            &CHECKS[..1]
        } else {
            &CHECKS
        };
        for (position, multiple) in multiples.iter().enumerate() {
            self.phase = if position == 0 {
                Phase::Random
            } else {
                Phase::Coverage
            };
            let outcome = match self.run_to(multiple * self.settings.cases)? {
                Some(outcome) => Some(outcome),
                None => self.tally.failure_without_counterexample(),
            };
            if outcome.is_some() {
                return Ok(outcome);
            }
            let last = position == multiples.len() - 1;
            let (shortfall, met) = self
                .tally
                .shortfall(requirements, last, self.tree.exhausted());
            if let Some(shortfall) = shortfall {
                let mut outcome = self.tally.outcome(Kind::CoverageUnmet, None);
                outcome.shortfall = Some(shortfall);
                return Ok(Some(outcome));
            }
            // The last check decides every requirement, so it ends the loop.
            if met {
                break;
            }
        }
        Ok(None)
    }
}

/// Runs the phases until the run ends, without the conclusion of a
/// counterexample.
fn explore(run: &mut Run<'_, '_>, settings: &mut Settings) -> Result<Outcome, Fault> {
    let mut tally = Tally::new(settings.seed);
    let mut stored = Vec::new();
    if let Some(mut outcome) = try_known(run, settings, &mut tally, &mut stored)? {
        outcome.stored = stored;
        return Ok(outcome);
    }
    let settings: &Settings = settings;
    let tally = &mut tally;
    let outcome = Executor::with(run, settings, |run, executor| {
        let mut phases = Phases {
            run,
            settings,
            tally: &mut *tally,
            executor,
            tree: Tree::default(),
            edges: 0,
            index: 0,
            phase: Phase::Random,
        };
        match phases.simplest()? {
            Some(outcome) => Ok(Some(outcome)),
            None => phases.checks(),
        }
    })?;
    let mut outcome = outcome.unwrap_or_else(|| tally.outcome(Kind::Passed, None));
    outcome.stored = stored;
    Ok(outcome)
}

/// Replays, shrinks and explains the failing case of a counterexample.
///
/// A shrink budget of 0 adds the token alone. A replay that differs makes the
/// outcome flaky. Any other outcome and a counterexample of values return
/// unchanged.
pub(crate) fn conclude(
    run: &mut Run<'_, '_>,
    settings: &Settings,
    mut outcome: Outcome,
) -> Result<Outcome, Fault> {
    if outcome.kind != Kind::Counterexample || outcome.valued {
        return Ok(outcome);
    }
    let failing = outcome
        .failing
        .take()
        .expect("a counterexample has its failing case");
    if settings.shrink == 0 {
        outcome.token = Some(token::encode(&failing.record.choices));
        outcome.failing = Some(failing);
        return Ok(outcome);
    }
    if let Some(divergence) = confirm(run, &failing, settings.max_choices)? {
        outcome.kind = Kind::Flaky;
        outcome.divergence = Some(divergence);
        outcome.failing = Some(failing);
        return Ok(outcome);
    }
    let identity = failing.identity().unwrap_or_default().to_owned();
    let mut shrinker = Shrinker::new(run, failing, settings);
    shrinker.shrink_all()?;
    let explanation = if settings.explain {
        explain(&mut shrinker, &identity, settings.seed)?
    } else {
        Vec::new()
    };
    let runs = shrinker.runs();
    let (first, others) = shrinker.into_failures(&identity);
    outcome.token = Some(token::encode(&first.record.choices));
    outcome.failing = Some(first);
    outcome.others = others;
    outcome.explanation = explanation;
    outcome.runs = runs;
    Ok(outcome)
}

/// Runs the one case that `choices` record, and reports it as found.
fn replay_only(
    run: &mut Run<'_, '_>,
    settings: &Settings,
    choices: Vec<Choice>,
) -> Result<Outcome, Fault> {
    let mut tally = Tally::new(settings.seed);
    let execution = run.execute(
        Box::new(Replaying::new(choices)),
        settings.max_choices,
        None,
    )?;
    (run.observer)(Phase::Token, &execution);
    if let Some(mut outcome) = tally.take(execution) {
        if let Some(failing) = &outcome.failing {
            outcome.token = Some(token::encode(&failing.record.choices));
        }
        return Ok(outcome);
    }
    Ok(tally
        .failure_without_counterexample()
        .unwrap_or_else(|| tally.outcome(Kind::Passed, None)))
}

/// Runs the phases and returns the outcome.
///
/// A run with a replay runs that one case instead, and neither shrinks nor
/// explains it.
///
/// # Errors
///
/// Returns the fault when a trace states an entry that the body cannot
/// follow, or a case ends the run with a fault.
pub(crate) fn run(run: &mut Run<'_, '_>, mut settings: Settings) -> Result<Outcome, Fault> {
    if let Some(choices) = settings.replay.take() {
        return replay_only(run, &settings, choices);
    }
    let explored = explore(run, &mut settings)?;
    conclude(run, &settings, explored)
}

/// Runs the stored cases from the oldest, and returns a pass that counts
/// them, or the outcome of the first stored case that does not pass, reported
/// as found with its token.
pub(crate) fn run_stored(run: &mut Run<'_, '_>, mut settings: Settings) -> Result<Outcome, Fault> {
    settings.traces.clear();
    settings.examples.clear();
    let mut tally = Tally::new(settings.seed);
    let mut stored = Vec::new();
    let found = try_known(run, &mut settings, &mut tally, &mut stored)?;
    let mut outcome = found.unwrap_or_else(|| tally.outcome(Kind::Passed, None));
    outcome.stored = stored;
    if let Some(failing) = &outcome.failing {
        outcome.token = Some(token::encode(&failing.record.choices));
    }
    Ok(outcome)
}

/// Returns the provider of random case `index` of `seed`.
pub(crate) fn generating(seed: u64, index: u64) -> Box<dyn Provider> {
    Box::new(Generating::new(case_source(seed, index)))
}

/// Returns the provider of an edge case.
pub(crate) fn edge(number: usize) -> Box<dyn Provider> {
    Box::new(Edge::new(BOUNDARIES[number]))
}
