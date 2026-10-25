// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! A campaign: a run that explores for a span of time instead of a number of
//! cases, and keeps the cases that show new behaviour.
//!
//! A campaign runs the known cases of a run first: the traces, the examples
//! and the stored cases. It then runs cases one at a time until its budget has
//! passed on the platform clock. A valid case joins the campaign's pool when
//! it counts a label that no earlier case counted, records a fingerprint that
//! no earlier case recorded, or records a score above the best of its label.
//!
//! Three cases in four of a campaign with a pool mutate a member of the pool,
//! chosen uniformly, and the fourth is the next random case of the seed. A
//! mutation draws one choice of the member again, deletes a span, repeats a
//! span after itself, or replaces a span with a span of the same label of a
//! member, each with odds of 1 in 4. A mutated case replays its choices, and
//! each request past them draws from the campaign's own stream. That stream
//! is the stream of index 2^64 − 1 of the seed, above the index of every
//! random case.
//!
//! The definition leaves these odds to each library.

use super::case::{AsAny, Drawn, Provider, Record, Request, Span};
use super::choice::{Bounds, Choice};
use super::draw::{Draw, below};
use super::execution::{Execution, Fault, Phase, Status};
use super::runner::{Kind, Outcome, Run, Settings, Tally, conclude, generating, known_cases};
use super::source::{Source, case_source};
use std::collections::{BTreeMap, HashSet};
use std::time::{Duration, Instant};

/// The odds that a case of a campaign with a pool mutates a member of the
/// pool: 3 in 4.
const MUTATE: (u128, u128) = (3, 4);

/// The kinds of mutation of a member, among which a mutated case chooses
/// uniformly: a choice drawn again, a span deleted, a span repeated, and the
/// fourth, a span replaced.
const REDRAW: u64 = 0;
const DELETE: u64 = 1;
const REPEAT: u64 = 2;
const KINDS: u128 = 4;

/// A case of the pool: its choices, the bounds of the request of each, and
/// its spans.
#[derive(Debug)]
struct Member {
    choices: Vec<Choice>,
    bounds: Vec<Bounds>,
    spans: Vec<Span>,
}

/// The provider of a mutated case. It replays the mutated choices, each
/// fitted to its request's bounds, and draws each request past them from the
/// campaign's stream.
#[derive(Debug)]
struct Mutating {
    choices: Vec<Choice>,
    source: Source,
}

impl Provider for Mutating {
    fn value(&mut self, request: &Request, index: usize) -> Choice {
        match self.choices.get(index) {
            Some(choice) => request.bounds.coerce(choice),
            None => request.draw.run(&mut self.source, &request.bounds),
        }
    }
}

/// The state of one campaign.
struct Exploration {
    tally: Tally,
    /// The campaign's own stream, of its decisions and of the requests past a
    /// mutated case's choices.
    source: Source,
    /// The index of the next random case of the seed.
    index: u64,
    pool: Vec<Member>,
    labels: HashSet<String>,
    fingerprints: HashSet<u64>,
    best: BTreeMap<String, f64>,
    /// The identities of the failures that the campaign concluded.
    identities: HashSet<String>,
    /// The first failure that the campaign concluded, with every later one
    /// among its others.
    found: Option<Outcome>,
    /// The draws of each stored case that ran.
    stored: Vec<Vec<Drawn>>,
}

impl Exploration {
    fn new(seed: u64) -> Self {
        Self {
            tally: Tally::new(seed),
            source: case_source(seed, u64::MAX),
            index: 0,
            pool: Vec::new(),
            labels: HashSet::new(),
            fingerprints: HashSet::new(),
            best: BTreeMap::new(),
            identities: HashSet::new(),
            found: None,
            stored: Vec::new(),
        }
    }

    /// Runs the next case of the exploration: a mutation of a member of the
    /// pool, or the next random case of the seed.
    fn next(&mut self, run: &Run<'_, '_>, settings: &Settings) -> Result<Execution, Fault> {
        if !self.pool.is_empty() && self.source.coin(MUTATE.0, MUTATE.1) {
            let mutated = Box::new(self.mutation());
            let mut execution = run.execute(mutated, settings.max_choices, None)?;
            // The campaign goes on from its stream after every value of the
            // mutated case.
            let provider: &dyn Provider = execution.record.provider();
            self.source = AsAny::as_any(provider)
                .downcast_ref::<Mutating>()
                .map_or(self.source, |mutating| mutating.source);
            return Ok(execution);
        }
        let execution = run.execute(
            generating(settings.seed, self.index),
            settings.max_choices,
            None,
        )?;
        self.index += 1;
        Ok(execution)
    }

    /// Returns the provider of a mutation of a member of the pool, chosen
    /// uniformly. A member without spans takes the mutation that draws a
    /// choice again, and so does a replacement whose donor has no span of the
    /// span's label. A member without choices takes no mutation, so every
    /// request of its case draws from the campaign's stream.
    fn mutation(&mut self) -> Mutating {
        let source = &mut self.source;
        let member = &self.pool[below(source, self.pool.len())];
        let mut choices = member.choices.clone();
        if choices.is_empty() {
            return Mutating {
                choices,
                source: *source,
            };
        }
        let drawn = source.below(KINDS);
        let mut kind = if member.spans.is_empty() {
            REDRAW
        } else {
            drawn
        };
        if kind != REDRAW {
            let span = &member.spans[below(source, member.spans.len())];
            let replaced = &member.choices[span.start..span.end];
            match kind {
                DELETE => {
                    choices.drain(span.start..span.end);
                }
                REPEAT => {
                    choices.splice(span.end..span.end, replaced.iter().cloned());
                }
                _ => {
                    let donor = &self.pool[below(source, self.pool.len())];
                    let same: Vec<&Span> = donor
                        .spans
                        .iter()
                        .filter(|other| other.label == span.label)
                        .collect();
                    if same.is_empty() {
                        kind = REDRAW;
                    } else {
                        let given = same[below(source, same.len())];
                        choices.splice(
                            span.start..span.end,
                            donor.choices[given.start..given.end].iter().cloned(),
                        );
                    }
                }
            }
        }
        if kind == REDRAW {
            let at = below(source, choices.len());
            choices[at] = Draw::Bounds.run(source, &member.bounds[at]);
        }
        Mutating {
            choices,
            source: *source,
        }
    }

    /// Counts an execution, admits a valid case to the pool, concludes a
    /// failure, and returns the outcome that ends the campaign: a skip before
    /// the first failure, with the draws of the stored cases, or the flaky
    /// outcome of a failure whose replay differs. `valued` states a case of an
    /// example of values.
    fn take(
        &mut self,
        run: &mut Run<'_, '_>,
        settings: &Settings,
        execution: Execution,
        valued: bool,
        concluded: &mut dyn FnMut(&Outcome),
    ) -> Result<Option<Outcome>, Fault> {
        if execution.status == Status::Passed {
            self.admit(&execution.record);
        }
        match self.tally.take(execution) {
            Some(mut skipped) if skipped.kind == Kind::Skipped => {
                if self.found.is_some() {
                    return Ok(None);
                }
                skipped.stored = std::mem::take(&mut self.stored);
                Ok(Some(skipped))
            }
            Some(found) => self.fail(run, settings, found, valued, concluded),
            None => Ok(None),
        }
    }

    /// Concludes the failure that `found` states when no earlier failure had
    /// its identity, and returns the flaky outcome of a replay that differs.
    /// The other failures that its shrink finds join its others, except those
    /// of an identity that the campaign concluded before.
    fn fail(
        &mut self,
        run: &mut Run<'_, '_>,
        settings: &Settings,
        mut found: Outcome,
        valued: bool,
        concluded: &mut dyn FnMut(&Outcome),
    ) -> Result<Option<Outcome>, Fault> {
        let identity = found
            .failing
            .as_ref()
            .and_then(Execution::identity)
            .unwrap_or_default()
            .to_owned();
        if self.identities.contains(&identity) {
            return Ok(None);
        }
        found.valued = valued;
        let mut outcome = conclude(run, settings, found)?;
        if outcome.kind == Kind::Flaky {
            let mut flaky = self.tally.outcome(Kind::Flaky, outcome.failing.take());
            flaky.divergence = outcome.divergence.take();
            flaky.stored = std::mem::take(&mut self.stored);
            return Ok(Some(flaky));
        }
        self.identities.insert(identity);
        let identities = &mut self.identities;
        outcome
            .others
            .retain(|other| identities.insert(other.identity().unwrap_or_default().to_owned()));
        concluded(&outcome);
        if let Some(first) = self.found.as_mut() {
            first.others.extend(outcome.failing);
            first.others.append(&mut outcome.others);
        } else {
            self.found = Some(outcome);
        }
        Ok(None)
    }

    /// Adds a valid case to the pool when it counts a label, records a
    /// fingerprint, or scores above the best of a label, for the first time.
    fn admit(&mut self, record: &Record) {
        let mut novel = false;
        for label in &record.labels {
            novel |= self.labels.insert(label.clone());
        }
        for fingerprint in &record.fingerprints {
            novel |= self.fingerprints.insert(*fingerprint);
        }
        for (label, score) in &record.targets {
            if self.best.get(label).is_none_or(|best| score > best) {
                self.best.insert(label.clone(), *score);
                novel = true;
            }
        }
        if novel {
            self.pool.push(Member {
                choices: record.choices.clone(),
                bounds: record
                    .requests
                    .iter()
                    .map(|request| request.bounds.clone())
                    .collect(),
                spans: record.spans.clone(),
            });
        }
    }

    /// Returns the outcome of a campaign whose budget has passed: the first
    /// failure that it concluded, with every later one among its others, or
    /// the outcome that the last check of a run decides over every valid case.
    fn result(mut self, settings: &Settings) -> Outcome {
        let mut outcome = if let Some(mut found) = self.found.take() {
            found.cases = self.tally.valid;
            found.rejected = self.tally.rejected;
            found
        } else if let Some(failed) = self.tally.failure_without_counterexample() {
            failed
        } else {
            let (shortfall, _) = self.tally.shortfall(&settings.requirements, true, false);
            let kind = if shortfall.is_some() {
                Kind::CoverageUnmet
            } else {
                Kind::Passed
            };
            let mut outcome = self.tally.outcome(kind, None);
            outcome.shortfall = shortfall;
            outcome
        };
        outcome.stored = self.stored;
        outcome
    }
}

/// Runs a campaign of the body of `run` under `settings`: the known cases of
/// a run, and then cases until `budget` has passed on the platform clock.
///
/// A failing case whose identity no earlier failure had is replayed, shrunk
/// and explained as a run concludes its failure, `concluded` receives the
/// outcome, and the campaign goes on. A replay that differs ends the campaign
/// as flaky. A case that skips before the first failure ends the campaign as
/// skipped. After the first failure, a case that skips counts as neither a
/// valid nor a rejected case, and the campaign goes on.
///
/// When the budget has passed, the campaign returns a counterexample of the
/// first failure that it concluded, with every later one among its others.
/// Without a failure, it returns the outcome that the last check of a run
/// decides over every valid case of the campaign. The outcome counts the
/// campaign's cases. The observer sees the random and the mutated cases under
/// the phase random.
///
/// # Errors
///
/// Returns the fault when a trace states an entry that the body cannot
/// follow, or a case ends the run with a fault.
pub(crate) fn campaign(
    run: &mut Run<'_, '_>,
    mut settings: Settings,
    budget: Duration,
    concluded: &mut dyn FnMut(&Outcome),
) -> Result<Outcome, Fault> {
    let end = Instant::now() + budget;
    let mut exploration = Exploration::new(settings.seed);
    for (phase, provider, valued) in known_cases(&mut settings) {
        let execution = run.execute(provider, settings.max_choices, None)?;
        (run.observer)(phase, &execution);
        if phase == Phase::Stored {
            exploration.stored.push(execution.record.draws.clone());
        }
        if let Some(ended) = exploration.take(run, &settings, execution, valued, concluded)? {
            return Ok(ended);
        }
    }
    while Instant::now() < end {
        let execution = exploration.next(run, &settings)?;
        (run.observer)(Phase::Random, &execution);
        if let Some(ended) = exploration.take(run, &settings, execution, false, concluded)? {
            return Ok(ended);
        }
    }
    Ok(exploration.result(&settings))
}
