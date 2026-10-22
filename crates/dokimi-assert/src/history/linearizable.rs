// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The check that a history is linearizable: that the calls of each of its
//! partitions have an order that respects their real-time order and that
//! the spec accepts.

use super::options::Options;
use super::partition::{
    Checked, Partition, Span, calls_of, concurrency, literals, partitions_of, span_json,
};
use super::recording::History;
pub(crate) use super::search::Verdict;
use super::search::{Ending, Limit, Search, SpecPanic};
use super::sentence;
use super::spec::Spec;
use crate::failure::{Failure, Where};
use crate::json::{Json, object};
use crate::prop::engine::canonical::literal_any;
use crate::seat::Seat;
use crate::value::Value;
use crate::verdict::{Mode, Run};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Instant;

/// The assertion of the record.
pub(crate) const LINEARIZABLE: &str = "linearizable";

/// How the search of one partition ended, or the panic of its spec, and
/// `None` for a partition whose search did not start.
type Searched<S> = Option<Result<Ending<S>, SpecPanic>>;

/// The detail of a check of a whole history, and the states after the order
/// that its search found.
type Whole<S> = (Detail<S>, Vec<S>);

/// The ten fields of the detail of a failing check, in order.
const FIELDS: [&str; 10] = [
    "outcome",
    "partitions",
    "steps",
    "partition",
    "calls",
    "concurrency",
    "linearized",
    "states",
    "candidates",
    "limit",
];

/// The detail of the record of a check. A check that passed states the
/// outcome, the partitions and the steps, and leaves the fields of a
/// reported partition empty.
#[derive(Debug, Clone)]
pub(crate) struct Detail<S> {
    pub(crate) outcome: Verdict,
    pub(crate) partitions: usize,
    /// The steps of the partitions up to and including the reported one, and
    /// of every partition of a check that passed.
    pub(crate) steps: u64,
    /// The keys of the reported partition, and none for every key.
    pub(crate) partition: Vec<Value>,
    pub(crate) calls: usize,
    pub(crate) concurrency: usize,
    pub(crate) linearized: Vec<Span>,
    pub(crate) states: Vec<S>,
    pub(crate) candidates: Vec<Span>,
    pub(crate) limit: Option<Limit>,
}

/// Returns the detail of a check that reports `partition`, whose search
/// ended as `ending`.
fn reported<S>(
    partition: &Partition,
    ending: Ending<S>,
    partitions: usize,
    steps: u64,
) -> Detail<S> {
    let spans = |positions: &[usize]| -> Vec<Span> {
        positions
            .iter()
            .map(|&at| partition.calls[at].span.clone())
            .collect()
    };
    Detail {
        outcome: ending.outcome,
        partitions,
        steps,
        partition: partition.keys.clone(),
        calls: partition.calls.len(),
        concurrency: concurrency(&partition.calls),
        linearized: spans(&ending.linearized),
        states: ending.states,
        candidates: spans(&ending.candidates),
        limit: ending.limit,
    }
}

/// Returns the detail of a check that passed.
fn passed<S>(partitions: usize, steps: u64) -> Detail<S> {
    Detail {
        outcome: Verdict::Passed,
        partitions,
        steps,
        partition: Vec::new(),
        calls: 0,
        concurrency: 0,
        linearized: Vec::new(),
        states: Vec::new(),
        candidates: Vec::new(),
        limit: None,
    }
}

/// Returns the endings of the searches of `partitions`, in their order, on
/// `options.workers` threads. A search that ended as violated ends the
/// searches of the partitions after it, which a check on one worker never
/// starts.
fn searched<S: Spec>(
    partitions: &[Partition],
    spec: &S,
    options: &Options,
    deadline: Option<Instant>,
) -> Vec<Searched<S::State>> {
    let search = |partition: &Partition| {
        Search::new(
            &partition.calls,
            spec,
            options.budget,
            options.memo_limit,
            deadline,
        )
        .run()
    };
    if options.workers == 1 || partitions.len() < 2 {
        let mut endings = Vec::with_capacity(partitions.len());
        for partition in partitions {
            let ending = search(partition);
            let stop = !matches!(&ending, Ok(ending) if ending.outcome != Verdict::Violated);
            endings.push(Some(ending));
            if stop {
                break;
            }
        }
        return endings;
    }
    let next = AtomicUsize::new(0);
    let violated = AtomicUsize::new(usize::MAX);
    let endings: Mutex<Vec<Searched<S::State>>> =
        Mutex::new(partitions.iter().map(|_| None).collect());
    thread::scope(|scope| {
        for _ in 0..options.workers.min(partitions.len()) {
            scope.spawn(|| {
                loop {
                    let at = next.fetch_add(1, Ordering::SeqCst);
                    if at >= partitions.len() || at > violated.load(Ordering::SeqCst) {
                        break;
                    }
                    let ending = search(&partitions[at]);
                    if !matches!(&ending, Ok(ending) if ending.outcome != Verdict::Violated) {
                        violated.fetch_min(at, Ordering::SeqCst);
                    }
                    endings
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)[at] = Some(ending);
                }
            });
        }
    });
    endings
        .into_inner()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Returns the detail of the check of `history` against `spec`: the first
/// violated partition, else the first undecided one, else a pass.
///
/// # Errors
///
/// Returns the panic of a function of the spec, in the first partition whose
/// search panicked before a violated one.
pub(crate) fn check<S: Spec>(
    history: &History,
    spec: &S,
    options: &Options,
) -> Result<Detail<S::State>, SpecPanic> {
    let deadline = (!options.time_limit.is_zero()).then(|| Instant::now() + options.time_limit);
    let (events, ids) = history.recorded();
    let partitions = partitions_of(calls_of(&events, &ids));
    let endings = searched(&partitions, spec, options, deadline);
    let mut steps = 0;
    let mut undecided = None;
    for (partition, ending) in partitions
        .iter()
        .zip(endings.into_iter().map_while(|ending| ending))
    {
        let ending = ending?;
        steps += ending.steps;
        match ending.outcome {
            Verdict::Passed => {}
            Verdict::Violated => return Ok(reported(partition, ending, partitions.len(), steps)),
            Verdict::Undecided => {
                if undecided.is_none() {
                    undecided = Some(reported(partition, ending, partitions.len(), steps));
                }
            }
        }
    }
    Ok(undecided.unwrap_or_else(|| passed(partitions.len(), steps)))
}

/// Returns the detail of the check of every call of `history` as one
/// partition, whatever keys the calls state, with the default limits, and
/// the states after the order that the search found: the initial state for a
/// history without calls, and none for a check that did not pass. A machine
/// checks the history of its steps this way, because its spec states one
/// state of the whole subject.
///
/// # Errors
///
/// Returns the panic of a function of the spec.
pub(crate) fn whole<S: Spec + ?Sized>(
    history: &History,
    spec: &S,
) -> Result<Whole<S::State>, SpecPanic> {
    let options = Options::default();
    let (events, ids) = history.recorded();
    let calls: Vec<Checked> = calls_of(&events, &ids);
    let ending = Search::new(&calls, spec, options.budget, options.memo_limit, None).run()?;
    if ending.outcome == Verdict::Passed {
        let partitions = usize::from(!calls.is_empty());
        return Ok((passed(partitions, ending.steps), ending.states));
    }
    let steps = ending.steps;
    let partition = Partition {
        keys: Vec::new(),
        calls,
    };
    Ok((reported(&partition, ending, 1, steps), Vec::new()))
}

/// Returns a state as a value of a record: its typed literal where its type
/// has one, and the text of its `Debug` otherwise.
pub(crate) fn state_value<S: std::fmt::Debug + 'static>(state: &S) -> Value {
    literal_any(state).unwrap_or_else(|| Value::debug(state))
}

/// Returns the detail of a failing check as its call record states it.
pub(crate) fn detail_json<S: std::fmt::Debug + 'static>(detail: &Detail<S>) -> Json {
    let states: Vec<Value> = detail.states.iter().map(state_value).collect();
    object([
        ("outcome", detail.outcome.name().into()),
        ("partitions", detail.partitions.into()),
        ("steps", Json::Int(i128::from(detail.steps))),
        ("partition", literals(&detail.partition)),
        ("calls", detail.calls.into()),
        ("concurrency", detail.concurrency.into()),
        (
            "linearized",
            Json::Array(detail.linearized.iter().map(span_json).collect()),
        ),
        ("states", literals(&states)),
        (
            "candidates",
            Json::Array(detail.candidates.iter().map(span_json).collect()),
        ),
        (
            "limit",
            detail.limit.map_or(Json::Null, |limit| limit.name().into()),
        ),
    ])
}

/// Returns the record of a failing check, whose detail `json` states, with
/// `msg` as its contract and its call site at `location`.
fn failure_of<S: std::fmt::Debug + 'static>(
    detail: &Detail<S>,
    json: &Json,
    msg: &str,
    location: Option<Where>,
) -> Failure {
    let fields = FIELDS
        .iter()
        .map(|name| {
            (
                *name,
                json.get(name).map_or(Value::Null, Value::from_detail),
            )
        })
        .collect();
    Failure::new(LINEARIZABLE, msg, fields, location)
        .with_sentence(sentence::linearizable(detail, msg))
}

/// Returns the record of a failing check, with `msg` as its contract and its
/// call site at `location`, as a machine's steps report it.
pub(crate) fn record<S: std::fmt::Debug + 'static>(
    detail: &Detail<S>,
    msg: &str,
    location: Option<Where>,
) -> Failure {
    failure_of(detail, &detail_json(detail), msg, location)
}

/// Fails when the calls of a partition of `history` have no order that
/// respects their real-time order and that `spec` accepts.
///
/// The check removes the calls that failed. A call that completed as ok is
/// known, and takes effect between its invocation and its completion. A
/// call whose outcome is unknown, and a pending call, takes effect at some
/// point after its invocation, or never. Two calls that share a key are in
/// one partition, and a call without keys puts every call into one
/// partition. The check searches the partitions in the order of their first
/// invocation, each from the spec's initial state, with the search that the
/// definition fixes. It reports the first violated partition, and otherwise
/// the first undecided one. An undecided check fails as a violated one does.
///
/// The record's detail states the ten fields of the definition: outcome,
/// partitions, steps, partition, calls, concurrency, linearized, states,
/// candidates and limit. A state of a type that has a typed literal states
/// it, and any other state states the text of its `Debug`. A function of the
/// spec that panics ends the call with a fault, which names the call that the
/// search stepped.
///
/// ```
/// use dokimi_assert::history::{self, History, Operation, Spec};
/// use dokimi_assert::{Collector, Value};
///
/// struct Register;
///
/// impl Spec for Register {
///     type State = Value;
///
///     fn initial(&self) -> Value {
///         Value::Null
///     }
///
///     fn next(&self, state: &Value, op: &Operation) -> Vec<Value> {
///         match op.name() {
///             "write" => op.args().to_vec(),
///             _ if op.returned(state.clone()) => vec![state.clone()],
///             _ => Vec::new(),
///         }
///     }
/// }
///
/// let h = History::new();
/// h.invoke(0, "write", [1.into()], ["x".into()]).ok(Value::Null);
/// h.invoke(1, "read", [], ["x".into()]).ok(1);
/// let t = Collector::new();
/// history::is_linearizable(&t, &h, &Register, "the register is linearizable", Default::default());
/// ```
#[track_caller]
pub fn is_linearizable<S: Spec>(
    seat: &dyn Seat,
    history: &History,
    spec: &S,
    msg: &str,
    options: Options,
) {
    seat.helper();
    let run = Run::of(seat);
    match check(history, spec, &options) {
        Err(panic) => {
            let (at, reason) = panic.place();
            let place = if at.is_empty() {
                String::new()
            } else {
                format!("{at}: ")
            };
            run.fault(
                Mode::Fatal,
                LINEARIZABLE,
                msg,
                format!("history::is_linearizable: {place}{reason}"),
            );
        }
        Ok(detail) if detail.outcome == Verdict::Passed => run.pass(Mode::Fatal, LINEARIZABLE, msg),
        Ok(detail) => {
            let json = detail_json(&detail);
            let failure = failure_of(&detail, &json, msg, Some(run.location()));
            run.fail_run(Mode::Fatal, &failure, json);
        }
    }
}
