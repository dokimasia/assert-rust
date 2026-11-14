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
//!   label and a value. Its value runs backwards through the draw's
//!   generator, and the draw decodes the choices that result. A draw past the
//!   last entry takes its targets.
//! - A draw of a composite takes no entry when it starts, and the draws of
//!   its function take theirs. Once it decodes, it takes the next entry by
//!   its label and does not read the value. A case records a composite after
//!   its draws, so the entry may lack a value.
//! - A draw through a composite, whose generator has a composite inside a
//!   one-of, an optional, a list, a dict, a filter or a recursive, takes the
//!   first draw entry with its label up to the next step entry. When the
//!   value of that entry runs backwards through a composite, the draws of the
//!   composite take the entries before it, and the draw takes the entry once
//!   it decodes. The entry must then be the next entry.
//! - The choices of a draw serve the requests of its own generator. A draw
//!   inside it serves its own requests, and the requests of the outer
//!   generator continue after it.
//! - A machine turns each step entry into the choices of one step, which the
//!   provider serves as they are requested.
//! - Every other request takes its target.
//!
//! A trace that the body cannot follow ends the run with a [`TraceError`],
//! which names the entry: a draw entry with another label, a draw entry
//! without a value, a value that the generator cannot produce, or a step
//! entry that the machine cannot take at its position. A draw inside a draw
//! through a composite that takes the entry of the outer draw makes the
//! outer draw refuse that entry. The error is no failure of the case.

use super::case::{Provider, Record, Request, Step};
use super::choice::Choice;
use super::inverse::{CannotInvert, Inverted};
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
        /// The value of the draw, or `None` for an entry that states no
        /// value.
        value: Option<Value>,
    },
    /// A step of a machine.
    Step(Step),
}

/// Why a body cannot follow an entry of a trace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// The draw has another label than the entry.
    Label,
    /// The entry states no value, or the draw's generator cannot produce the
    /// entry's value.
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

/// A draw in progress, with the choices of its requests and the way it takes
/// its entry.
#[derive(Debug, Default)]
struct Open {
    /// The choices of the draw that no request has taken.
    queue: VecDeque<Choice>,
    /// The position of the entry that a draw through a composite takes once
    /// it decodes, and `None` for every other draw.
    end: Option<usize>,
    /// Whether the draw is of a composite, which takes the next entry by its
    /// label alone once it decodes.
    composite: bool,
}

/// A provider that serves the values of a trace's entries.
#[derive(Debug)]
pub struct Tracing {
    entries: Vec<Entry>,
    at: usize,
    /// The draws in progress, the innermost last. The first serves the
    /// requests outside every draw, such as the steps of a machine.
    open: Vec<Open>,
}

/// Returns the refusal of the entry at `entry` by the draw labelled `label`.
fn refused(entry: usize, label: &str, refusal: Refusal) -> TraceError {
    TraceError {
        entry,
        name: label.to_owned(),
        refusal,
    }
}

impl Tracing {
    /// Returns the provider of `entries`, from the first.
    #[must_use]
    pub fn new(entries: Vec<Entry>) -> Self {
        Self {
            entries,
            at: 0,
            open: vec![Open::default()],
        }
    }

    /// Opens the draw of a composite, which does not take an entry until it
    /// decodes.
    pub(crate) fn composite(&mut self) {
        self.open.push(Open {
            composite: true,
            ..Open::default()
        });
    }

    /// Opens a draw labelled `label` with the choices that `invert` returns
    /// for the value of its entry. A draw through a composite, for which
    /// `through` is `true`, takes the first draw entry with `label` up to the
    /// next step entry. When `invert` reports a value through a composite,
    /// the draw takes that entry once it decodes. Otherwise the draw takes
    /// the next entry now. A draw past the last entry does not take an entry.
    ///
    /// # Errors
    ///
    /// Returns an error when the entry is no draw entry with `label`, when it
    /// states no value, or when the generator cannot produce its value. A
    /// value without a composite in a later entry is refused at the next
    /// entry.
    pub(crate) fn drawing(
        &mut self,
        label: &str,
        through: bool,
        invert: &dyn Fn(&Value) -> Result<Inverted, CannotInvert>,
    ) -> Result<(), TraceError> {
        if self.at >= self.entries.len() {
            self.open.push(Open::default());
            return Ok(());
        }
        let at = if through { self.find(label) } else { self.at };
        let stated = match &self.entries[at] {
            Entry::Draw {
                label: stated,
                value,
            } if stated == label => value.as_ref(),
            _ => return Err(refused(self.at, label, Refusal::Label)),
        };
        let Some(value) = stated else {
            return Err(refused(at, label, Refusal::Value));
        };
        let (choices, composite) = invert(value).map_err(|_| refused(at, label, Refusal::Value))?;
        if composite {
            self.open.push(Open {
                queue: choices.into(),
                end: Some(at),
                composite: false,
            });
            return Ok(());
        }
        if at != self.at {
            return Err(refused(self.at, label, Refusal::Label));
        }
        self.at += 1;
        self.open.push(Open {
            queue: choices.into(),
            ..Open::default()
        });
        Ok(())
    }

    /// Ends the innermost open draw, labelled `label`, and takes its entry
    /// where the draw takes one once it decodes. A composite takes the next
    /// entry by its label alone, and it does not take an entry past the last
    /// entry. A draw through a composite takes its entry once the draws inside
    /// it have taken the entries before it. That entry must be the next
    /// entry.
    ///
    /// # Errors
    ///
    /// Returns an error when the next entry is no draw entry with `label`,
    /// and when it is not the entry of the draw through a composite. When a
    /// draw inside took that entry, the error is at that entry.
    pub(crate) fn drawn(&mut self, label: &str) -> Result<(), TraceError> {
        let ended = self.open.pop().unwrap_or_default();
        if ended.composite {
            return self.take_draw(label);
        }
        match ended.end {
            Some(end) if self.at != end => Err(refused(self.at.min(end), label, Refusal::Label)),
            Some(_) => {
                self.at += 1;
                Ok(())
            }
            None => Ok(()),
        }
    }

    /// Takes the next entry, which must be a draw entry with `label`. A draw
    /// past the last entry does not take an entry.
    fn take_draw(&mut self, label: &str) -> Result<(), TraceError> {
        match self.entries.get(self.at) {
            None => Ok(()),
            Some(Entry::Draw { label: stated, .. }) if stated == label => {
                self.at += 1;
                Ok(())
            }
            Some(_) => Err(refused(self.at, label, Refusal::Label)),
        }
    }

    /// Returns the position of the first draw entry with `label` from the
    /// next entry up to the next step entry, and the position of the next
    /// entry when there is none.
    fn find(&self, label: &str) -> usize {
        self.entries[self.at..]
            .iter()
            .take_while(|entry| matches!(entry, Entry::Draw { .. }))
            .position(|entry| matches!(entry, Entry::Draw { label: stated, .. } if stated == label))
            .map_or(self.at, |offset| self.at + offset)
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

    /// Serves integer values to the next requests of the innermost open draw,
    /// before any target.
    pub(crate) fn prepare(&mut self, values: &[i128]) {
        self.innermost()
            .extend(values.iter().map(|value| Choice::Integer(*value)));
    }

    /// Returns the queue of the innermost open draw. The bottom of the stack
    /// serves the requests outside every draw, so the stack is never empty.
    fn innermost(&mut self) -> &mut VecDeque<Choice> {
        &mut self
            .open
            .last_mut()
            .expect("the requests outside every draw are open")
            .queue
    }

    /// Takes the next entry, a step entry, and serves the values of its
    /// choices.
    pub(crate) fn take(&mut self, values: &[i128]) {
        self.prepare(values);
        self.at += 1;
    }

    /// Returns the position of the first entry that the case left untaken,
    /// or `None` when it took every entry.
    pub(crate) fn untaken(&self) -> Option<usize> {
        (self.at < self.entries.len()).then_some(self.at)
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
        match self.innermost().pop_front() {
            Some(prepared) => request.bounds.coerce(&prepared),
            None => request.bounds.target(),
        }
    }

    fn tracing(&mut self) -> Option<&mut Tracing> {
        Some(self)
    }
}
