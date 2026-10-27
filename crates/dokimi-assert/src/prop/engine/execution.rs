// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! One call of a body, and how it ended.
//!
//! The runner, the shrinker and the explain phase each call the body through
//! `execute`. Every case runs outside the case tree and keeps its walk, and
//! `entered` walks a case into the tree once it has ended. The walk is the
//! walk of the case on one worker, so it stops where a run on one worker
//! stops the case: at a choice that repeats a tested case, or at a request
//! that differs from the recorded one. The calls that the case made after
//! that step are cut from its record.
//!
//! A body can make its case run more than once: the steps of a machine do
//! when the clients of a concurrent section run on threads, which do not
//! replay. Each later run replays the choices of the case, and the first run
//! that fails or skips ends the case.
//!
//! A case that skips leaves no leaf in the tree. Before the run finds a
//! failing case the skip ends the run, and after it the case counts as a case
//! that does not fail.

use super::case::{CaseFault, Failed, Place, Provider, Record, Replaying, Stop, Where};
use super::choice::Bounds;
use super::trace::TraceError;
use super::tree::{Ending, Stopped, Tree, Walker};
use crate::matcher::panics::{last_location, message, quietly};
use crate::prop::Case;
use crate::prop::case::Context;
use crate::record::Calls;
use std::sync::Arc;

pub use crate::record::Phase;

/// How one case ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// The body returned, and nothing failed.
    Passed,
    /// The body failed.
    Failed,
    /// The case states no input of the domain, or passed its cap.
    Rejected,
    /// The case repeats a tested case.
    Repeated,
    /// The body requested other choices after the same values.
    Diverged,
    /// The case skipped the run of its property.
    Skipped,
}

/// One side of a divergence: the bounds of a request, a fingerprint, the
/// identity of a failure, or nothing.
#[derive(Debug, Clone, PartialEq)]
pub enum Version {
    /// The bounds of a request.
    Bounds(Bounds),
    /// A fingerprint.
    Fingerprint(u64),
    /// The identity of a failure.
    Identity(String),
    /// No request, no fingerprint or a pass.
    Nothing,
}

/// The first difference between two runs of the same choices.
///
/// `what` is `request` for the bounds of a request, `fingerprint` for an
/// observed fingerprint, and `verdict` for a replay that passed or failed
/// another way. `index` is the position of the request or the fingerprint that
/// differs, and for a verdict the number of choices of the recorded case.
/// `label` and `step` state where the replay made the request or observed the
/// fingerprint.
#[derive(Debug, Clone, PartialEq)]
pub struct Divergence {
    /// The kind of difference.
    pub what: &'static str,
    /// The position of the difference.
    pub index: usize,
    /// The recorded side.
    pub recorded: Version,
    /// The replayed side.
    pub replayed: Version,
    /// The label of the draw that was running.
    pub label: Option<String>,
    /// The part and the step of a machine.
    pub step: Option<Place>,
}

impl Divergence {
    pub(crate) fn at(
        what: &'static str,
        index: usize,
        recorded: Version,
        replayed: Version,
        place: &Where,
    ) -> Self {
        Self {
            what,
            index,
            recorded,
            replayed,
            label: place.label.clone(),
            step: place.place.clone(),
        }
    }
}

/// One call of the body: its case and how it ended.
#[derive(Debug)]
pub struct Execution {
    pub(crate) record: Record,
    pub(crate) status: Status,
    pub(crate) failure: Option<Failed>,
    pub(crate) divergence: Option<Divergence>,
    pub(crate) overran: bool,
    /// The calls of each run of the body, for a recorded property.
    pub(crate) calls: Vec<Option<Arc<Calls>>>,
    /// The step at which the walk into the tree stopped the case. A run on
    /// one worker does not make the calls that the case made after it.
    pub(crate) stopped: Option<usize>,
    /// Whether the case ran more than once.
    pub(crate) repeated: bool,
    /// The reason of a case that skipped.
    pub(crate) reason: Option<String>,
}

impl Execution {
    /// Returns how the case ended.
    #[must_use]
    pub fn status(&self) -> Status {
        self.status
    }

    /// Returns the identity of the failure, or `None` when the case did not
    /// fail.
    #[must_use]
    pub fn identity(&self) -> Option<&str> {
        self.failure
            .as_ref()
            .map(|failure| failure.identity.as_str())
    }
}

/// What ends a run before its outcome: an entry of a trace that the body
/// cannot follow, a fault of the run, or a fault of a call of the body.
#[derive(Debug, Clone, PartialEq)]
pub enum Fault {
    /// A trace's entry that the body cannot follow.
    Trace(TraceError),
    /// A fault of the run, with its text.
    Run(String),
    /// A fault that ended a call of the body, in any phase of the run.
    Case(CaseFault),
}

/// A body of a property.
pub(crate) type Body<'b, 's> = &'b (dyn Fn(&Case<'s>) + Sync);

/// Calls the body once on a case whose values come from `provider`, and
/// returns how the case ended.
///
/// The body and each cleanup run with the panic hook silenced. A panic that
/// is no signal of the case fails the case at the panic's location. A case
/// that failed ends as failed, also when it was rejected, skipped or passed its
/// cap. A case that asks to run more than once runs at most `limit` times, and
/// the first later run that fails or skips ends it.
pub(crate) fn execute<'s>(
    body: Body<'_, 's>,
    provider: Box<dyn Provider>,
    max_choices: usize,
    context: Option<&'s Context<'s>>,
    limit: Option<usize>,
) -> Result<Execution, Fault> {
    let first = once(body, Case::new(Record::new(provider, max_choices), context))?;
    let runs = limit.map_or(first.record.repeats, |limit| {
        first.record.repeats.min(limit)
    });
    if first.status != Status::Passed || runs <= 1 {
        return Ok(first);
    }
    let mut calls = first.calls.clone();
    for _ in 1..runs {
        let replayed = Box::new(Replaying::new(first.record.choices.clone()));
        let later = once(body, Case::new(Record::new(replayed, max_choices), context))?;
        calls.extend(later.calls.iter().cloned());
        if matches!(later.status, Status::Failed | Status::Skipped) {
            return Ok(Execution {
                calls,
                repeated: true,
                ..later
            });
        }
    }
    Ok(Execution {
        calls,
        repeated: true,
        ..first
    })
}

/// Calls the body once on `case`, runs the cleanups, and ends the case.
fn once<'s>(body: Body<'_, 's>, case: Case<'s>) -> Result<Execution, Fault> {
    let mut ended: Option<Stop> = None;
    let mut settle = |outcome: std::thread::Result<()>| {
        let Err(payload) = outcome else { return };
        match payload.downcast::<super::case::Signal>() {
            Ok(signal) => {
                if ended.is_none() {
                    ended = Some(signal.0);
                }
            }
            Err(payload) => {
                case.panicked(&message(&*payload), last_location());
            }
        }
    };
    settle(quietly(|| body(&case)));
    for cleanup in case.take_cleanups() {
        settle(quietly(cleanup));
    }
    case.finish();
    match &ended {
        Some(Stop::Trace(refused)) => return Err(Fault::Trace(refused.clone())),
        Some(Stop::Fault(text)) => return Err(Fault::Run(text.clone())),
        Some(Stop::Faulted(fault)) => return Err(Fault::Case(fault.clone())),
        _ => {}
    }
    // A case keeps every failure before a stop fails it, so a stop of a
    // failure always follows a kept one.
    let failure = case.failure();
    let calls = case.calls_of();
    let (record, _) = case.into_parts();
    let (status, overran, reason) = match (&failure, ended) {
        (Some(_), _) => (Status::Failed, false, None),
        (None, Some(Stop::Overrun)) => (Status::Rejected, true, None),
        (None, Some(Stop::Skipped(reason))) => (Status::Skipped, false, Some(reason)),
        (None, Some(_)) => (Status::Rejected, false, None),
        (None, None) => (Status::Passed, false, None),
    };
    Ok(Execution {
        record,
        status,
        failure,
        divergence: None,
        overran,
        calls: vec![calls],
        stopped: None,
        repeated: false,
        reason,
    })
}

/// Walks a case that ran outside the tree into the tree, and returns its
/// execution in the tree and the number of steps of its walk.
///
/// The walk takes every choice that the first run made, also the choices
/// that a rewind removed from the record, because a case that walks the tree
/// as it runs steps them too. A stopped case keeps no later run, because a run
/// on one worker does not make it. A case that the walk does not stop ends as
/// its first run did. It has a leaf unless it passed its cap or skipped.
pub(crate) fn entered(tree: &mut Tree, mut execution: Execution) -> (Execution, usize) {
    let mut walker = Walker::new();
    let steps = execution.record.walk.len();
    for number in 0..steps {
        let step = &execution.record.walk[number];
        match walker.step(tree, step.index, &step.request, &step.value) {
            Ok(()) => {}
            Err(Stopped::Repeated) => {
                execution.status = Status::Repeated;
                execution.failure = None;
                execution.stopped = Some(number + 1);
                execution.calls.truncate(1);
                return (execution, number + 1);
            }
            Err(Stopped::Diverged {
                index,
                recorded,
                requested,
            }) => {
                let place = step.place.clone();
                execution.status = Status::Diverged;
                execution.failure = None;
                execution.divergence = Some(Divergence::at(
                    "request",
                    index,
                    Version::Bounds(recorded),
                    version(requested),
                    &place,
                ));
                execution.stopped = Some(number + 1);
                execution.calls.truncate(1);
                return (execution, number + 1);
            }
        }
    }
    if !execution.overran && execution.status != Status::Skipped {
        let ending = match execution.status {
            Status::Failed if !execution.repeated => Ending::Failed,
            Status::Rejected => Ending::Rejected,
            _ => Ending::Passed,
        };
        if let Err(Stopped::Diverged {
            index,
            recorded,
            requested,
        }) = walker.end(tree, ending)
        {
            execution.status = Status::Diverged;
            execution.failure = None;
            execution.divergence = Some(Divergence::at(
                "request",
                index,
                Version::Bounds(recorded),
                version(requested),
                &Where::default(),
            ));
        }
    }
    (execution, steps)
}

fn version(bounds: Option<Bounds>) -> Version {
    bounds.map_or(Version::Nothing, Version::Bounds)
}
