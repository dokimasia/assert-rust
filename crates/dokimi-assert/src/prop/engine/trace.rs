// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! Traces: a case stated as the draws that it makes and the machine steps
//! that it takes.
//!
//! A trace lists entries in request order. A draw entry states a label and a
//! value. A step entry names an action, and states its client in a
//! concurrent section and the drain mark in the drain. `entries` returns
//! the trace of a case, which is how a machine's counterexample lists it.
//!
//! A body runs on a trace through [`Tracing`], a provider whose values come
//! from the entries:
//!
//! - A draw takes the next entry, which must be a draw entry with the draw's
//!   label. Its value runs backwards through the draw's generator, and the
//!   draw decodes the choices that result. A draw past the last entry takes
//!   its targets.
//! - A machine turns each step entry into the choices of one step, which the
//!   provider serves as they are requested.
//! - Every other request takes its target.
//!
//! A trace that the body cannot follow ends the run with a [`TraceError`],
//! which names the entry: a draw entry with another label, a value that the
//! generator cannot produce, or a step entry that the machine cannot take at
//! its position. The error is no failure of the case.

use super::case::{Provider, Record, Request, Step};
use super::choice::Choice;
use super::inverse::CannotInvert;
use crate::value::Value;
use std::collections::{BTreeSet, VecDeque};
use std::fmt;

/// One entry of a trace.
#[derive(Debug, Clone, PartialEq)]
pub enum Entry {
    /// A draw: its label and the value that it decoded.
    Draw {
        /// The label of the draw.
        label: String,
        /// The value of the draw.
        value: Value,
    },
    /// A step of a machine.
    Step(Step),
}

/// Why a body cannot follow an entry of a trace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// The draw has another label than the entry.
    Label,
    /// The draw's generator cannot produce the entry's value.
    Value,
    /// The machine cannot take the step entry at its position.
    Step,
}

impl Refusal {
    /// Returns the name of the refusal, as a vector states it.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Label => "label",
            Self::Value => "value",
            Self::Step => "step",
        }
    }
}

/// An entry that the body cannot follow, named by its position.
///
/// `name` is the draw's label for a refusal of the label or the value, and
/// the step's action for a refusal of the step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceError {
    /// The position of the entry, from 0.
    pub entry: usize,
    /// The draw's label or the step's action.
    pub name: String,
    /// What the body cannot follow.
    pub refusal: Refusal,
}

impl fmt::Display for TraceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "prop: trace entry {} ({}): {}",
            self.entry,
            self.name,
            self.refusal.name()
        )
    }
}

impl std::error::Error for TraceError {}

/// Returns a case's draws and steps as a trace, in request order. A step
/// comes before the draws that the case recorded after it.
pub(crate) fn entries(record: &Record) -> Vec<Traced> {
    let mut trace = Vec::with_capacity(record.draws.len() + record.steps.len());
    let mut steps = record.steps.iter().peekable();
    for number in 0..record.draws.len() {
        while let Some((_, step)) = steps.next_if(|(before, _)| *before <= number) {
            trace.push(Traced::Step(step.clone()));
        }
        trace.push(Traced::Draw(number));
    }
    trace.extend(steps.map(|(_, step)| Traced::Step(step.clone())));
    trace
}

/// One entry of a case's trace: a step, or the draw at a position of the
/// case's draws.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Traced {
    Draw(usize),
    Step(Step),
}

/// A provider that serves the values of a trace's entries.
#[derive(Debug)]
pub struct Tracing {
    entries: Vec<Entry>,
    at: usize,
    queue: VecDeque<Choice>,
}

impl Tracing {
    /// Returns the provider of `entries`, from the first.
    #[must_use]
    pub fn new(entries: Vec<Entry>) -> Self {
        Self {
            entries,
            at: 0,
            queue: VecDeque::new(),
        }
    }

    /// Prepares the choices of the next entry for a draw labelled `label`,
    /// whose generator's inverse is `invert`.
    ///
    /// # Errors
    ///
    /// Returns an error when the next entry is no draw entry with `label`, or
    /// when the generator cannot produce its value.
    pub(crate) fn drawing(
        &mut self,
        label: &str,
        invert: &dyn Fn(&Value) -> Result<Vec<Choice>, CannotInvert>,
    ) -> Result<(), TraceError> {
        let Some(entry) = self.entries.get(self.at) else {
            return Ok(());
        };
        let refused = |refusal| TraceError {
            entry: self.at,
            name: label.to_owned(),
            refusal,
        };
        let Entry::Draw {
            label: stated,
            value,
        } = entry
        else {
            return Err(refused(Refusal::Label));
        };
        if stated != label {
            return Err(refused(Refusal::Label));
        }
        let choices = invert(value).map_err(|_| refused(Refusal::Value))?;
        self.queue.extend(choices);
        self.at += 1;
        Ok(())
    }

    /// Returns the next entry when it is a step entry.
    pub(crate) fn next_step(&self) -> Option<&Step> {
        match self.entries.get(self.at) {
            Some(Entry::Step(step)) => Some(step),
            _ => None,
        }
    }

    /// Returns the actions that the step entries from the next one on name.
    pub(crate) fn actions(&self) -> BTreeSet<String> {
        self.entries[self.at.min(self.entries.len())..]
            .iter()
            .filter_map(|entry| match entry {
                Entry::Step(step) => Some(step.action.clone()),
                Entry::Draw { .. } => None,
            })
            .collect()
    }

    /// Serves integer values to the next requests, before any target.
    pub(crate) fn prepare(&mut self, values: &[i128]) {
        self.queue
            .extend(values.iter().map(|value| Choice::Integer(*value)));
    }

    /// Takes the next entry, a step entry, and serves the values of its
    /// choices.
    pub(crate) fn take(&mut self, values: &[i128]) {
        self.prepare(values);
        self.at += 1;
    }

    /// Returns the error of the next entry, a step entry that the machine
    /// cannot take.
    pub(crate) fn refuse(&self) -> TraceError {
        TraceError {
            entry: self.at,
            name: self
                .next_step()
                .map(|step| step.action.clone())
                .unwrap_or_default(),
            refusal: Refusal::Step,
        }
    }
}

impl Provider for Tracing {
    fn value(&mut self, request: &Request, _index: usize) -> Choice {
        match self.queue.pop_front() {
            Some(prepared) => request.bounds.coerce(&prepared),
            None => request.bounds.target(),
        }
    }

    fn tracing(&mut self) -> Option<&mut Tracing> {
        Some(self)
    }
}
