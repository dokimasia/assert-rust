// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The run of a stored case: its choices first, and the counterexample of
//! its entry as a trace when the choices decode to another counterexample.
//!
//! The case of the choices states the entry's counterexample when its own
//! counterexample has the same entries in order: the draw entries have the
//! same labels, and the same values where the entry records one, as two
//! typed literals compare. For an entry of format 2, the step entries have
//! the same actions, clients and drain marks. An entry of format 1 records no
//! step entries, so the steps of its case do not count.

use super::canonical::same;
use super::case::{Drawn, Record, Replaying};
use super::choice::Choice;
use super::execution::{Execution, Fault, Phase, Status};
use super::runner::Run;
use super::store::step_of;
use super::trace::{self, Entry, TraceError, Traced, Tracing};
use crate::json::Json;
use crate::value::Value;

/// A stored case. It has the choices of its entry, the counterexample that
/// its entry records, and whether the entry is of format 2.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct StoredCase {
    /// The choices of the entry.
    pub(crate) choices: Vec<Choice>,
    /// The entries of the counterexample of the entry, in request order.
    pub(crate) counterexample: Vec<Json>,
    /// Whether the entry records the steps of a machine, as an entry of
    /// format 2 does.
    pub(crate) steps: bool,
}

/// How a run took a stored case.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Taken {
    /// The case of the choices states the counterexample of the entry.
    Stated,
    /// The case of the choices states another counterexample, and the case
    /// of the trace counts.
    Replayed,
    /// The case of the choices states another counterexample and counts,
    /// because it fails.
    Failed,
    /// The body refused an entry of the trace, so the case of the choices
    /// counts.
    Refused(TraceError),
    /// The case of the trace left the entry at this position untaken, so the
    /// case of the choices counts.
    Untaken(usize),
}

/// Runs `stored` under the phase stored, and returns the case that counts
/// for it with how the run took it.
///
/// The case of the choices counts when it states the counterexample of the
/// entry or fails. Any other case of the choices gives way to the case of
/// the trace, which counts when it fails or takes every entry. The case of
/// the choices counts when the body refuses an entry of the trace, and when
/// the case of the trace leaves an entry untaken. The observer receives the
/// case of the choices, and the case of the trace when that case counts.
pub(crate) fn run_stored(
    run: &mut Run<'_, '_>,
    stored: &StoredCase,
    max_choices: usize,
) -> Result<(Execution, Taken), Fault> {
    let replaying = Box::new(Replaying::new(stored.choices.clone()));
    let execution = run.execute(replaying, max_choices, None)?;
    (run.observer)(Phase::Stored, &execution);
    if states(stored, &execution.record) {
        return Ok((execution, Taken::Stated));
    }
    if execution.status == Status::Failed {
        return Ok((execution, Taken::Failed));
    }
    let tracing = Box::new(Tracing::new(trace_of(&stored.counterexample)));
    let mut traced = match run.execute(tracing, max_choices, None) {
        Ok(traced) => traced,
        Err(Fault::Trace(error)) => return Ok((execution, Taken::Refused(error))),
        Err(fault) => return Err(fault),
    };
    let untaken = traced
        .record
        .provider()
        .tracing()
        .and_then(|tracing| tracing.untaken());
    match untaken {
        Some(at) if traced.status != Status::Failed => Ok((execution, Taken::Untaken(at))),
        _ => {
            (run.observer)(Phase::Stored, &traced);
            Ok((traced, Taken::Replayed))
        }
    }
}

/// Returns the counterexample that an entry records as a trace. A draw entry
/// without a value, or with a value that does not decode as a typed literal,
/// has no value in the trace.
pub(crate) fn trace_of(counterexample: &[Json]) -> Vec<Entry> {
    counterexample
        .iter()
        .filter_map(|entry| match entry.get("label").and_then(Json::as_str) {
            Some(label) => Some(Entry::Draw {
                label: label.to_owned(),
                value: entry
                    .get("value")
                    .and_then(|value| Value::from_literal(value).ok()),
            }),
            None => step_of(entry).map(Entry::Step),
        })
        .collect()
}

/// Returns `true` when the case of `record` states the counterexample of
/// `stored`.
fn states(stored: &StoredCase, record: &Record) -> bool {
    let entries: Vec<Traced> = trace::entries(record)
        .into_iter()
        .filter(|entry| stored.steps || matches!(entry, Traced::Draw(_)))
        .collect();
    entries.len() == stored.counterexample.len()
        && entries
            .iter()
            .zip(&stored.counterexample)
            .all(|(entry, recorded)| match entry {
                Traced::Draw(number) => draws(&record.draws[*number], recorded),
                Traced::Step(step) => step_of(recorded).as_ref() == Some(step),
            })
}

/// Returns `true` when a draw has the label of a recorded entry, and its
/// value where the entry records one.
fn draws(drawn: &Drawn, recorded: &Json) -> bool {
    if recorded.get("label").and_then(Json::as_str) != Some(drawn.label.as_str()) {
        return false;
    }
    let Some(stated) = recorded.get("value") else {
        return true;
    };
    match (Value::from_literal(stated).ok(), &drawn.value.literal) {
        (Some(stated), Some(value)) => same(&stated, value),
        _ => false,
    }
}
