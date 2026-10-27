// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The record of the choices and the spans of one call of a body.
//!
//! A generator never reads the random source itself. It asks the case for a
//! choice with bounds, and the provider of the case supplies the value:
//!
//! - A generating provider draws the value from the source.
//! - A replaying provider reads the value back from recorded choices.
//! - The edge phase gives the value.
//! - The fuzz bridge decodes the value from the bytes of a fuzzer.
//!
//! The case records every value with its request and the spans of the
//! generators that asked for it. The shrinker edits that record.

use super::choice::{Bounds, Choice, IntegerBounds, SequenceBounds};
use super::draw::{Draw, REUSE_ODDS};
use super::source::Source;
use super::trace::{TraceError, Tracing};
use crate::failure::{Failure, Where as CallSite};
use crate::value::Value;
use std::any::Any;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;
use std::sync::Arc;

/// The most choices that one case may make. A sequence counts as one choice
/// plus one for each element.
pub const MAX_CHOICES: usize = 8192;

/// One choice that a generator asks for.
///
/// A choice that decides the structure of what its generator returns, such
/// as a collection's continue flag or a one-of's index, states `edge`: the
/// value that the edge phase gives it. A value choice has no edge. `reuse`
/// marks a value of the integer and duration generators, which a generated
/// case may take from an earlier choice with the same bounds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    /// The bounds of the choice.
    pub bounds: Bounds,
    /// How a generated case draws the value.
    pub draw: Draw,
    /// The value that the edge phase gives a structure choice.
    pub edge: Option<i128>,
    /// Whether a generated case may take the value of an earlier choice with
    /// the same bounds.
    pub reuse: bool,
}

impl Request {
    /// Returns the request of a value inside `bounds`, drawn as the bounds'
    /// kind draws.
    #[must_use]
    pub fn of(bounds: Bounds) -> Self {
        Self {
            bounds,
            draw: Draw::Bounds,
            edge: None,
            reuse: false,
        }
    }
}

/// The step of a machine that a case took: its action, and where it ran.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    /// The name of the action.
    pub action: String,
    /// The client of a step of a concurrent section.
    pub client: Option<usize>,
    /// Whether the step is a step of the drain.
    pub drain: bool,
}

/// The part of a machine's steps that was running, and its step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Place {
    /// The part: swarm, setup, sequential, concurrent, drain or settle.
    pub part: &'static str,
    /// The position of the step in its part, from 0.
    pub position: Option<usize>,
    /// The action of the swarm choice or of the step.
    pub action: Option<String>,
}

/// Where a case made a request or observed a fingerprint.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Where {
    /// The label of the innermost draw that was running.
    pub label: Option<String>,
    /// The machine's part and step.
    pub place: Option<Place>,
}

/// One step of the walk of a case: a choice, with the length that the record
/// had before it.
#[derive(Debug, Clone)]
pub(crate) struct Walked {
    pub(crate) index: usize,
    pub(crate) request: Request,
    pub(crate) value: Choice,
    pub(crate) place: Where,
}

/// The choices `[start, end)` that one generator, element or entry made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    /// The id of the generator, or `element` or `entry`.
    pub label: &'static str,
    /// The first choice.
    pub start: usize,
    /// The choice after the last.
    pub end: usize,
    /// The spans that were open when this one opened.
    pub depth: usize,
    /// The span that was open when this one opened.
    pub parent: Option<usize>,
}

/// A point of the record that a rewind returns to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Mark {
    choices: usize,
    spans: usize,
    draws: usize,
}

/// A value as a case's report shows it: the text of its `Debug`, and its
/// typed literal where it has one.
#[derive(Debug, Clone, PartialEq)]
pub struct Rendered {
    /// The text of the value.
    pub text: String,
    /// The typed literal of the value.
    pub literal: Option<Value>,
}

/// A draw's generator, with its type erased, as the explain phase uses it.
pub(crate) trait Erased: Send + Sync {
    /// Decodes a value from `case` and drops it.
    fn fill(&self, case: &crate::prop::Case<'_>) -> Result<(), Stop>;
    /// Returns the integer bounds of an integer draw, through maps.
    fn integer(&self) -> Option<IntegerBounds>;
    /// Returns the value that an integer draw decodes from `choice`.
    fn nearest(&self, choice: i128) -> Option<Rendered>;
}

/// One value that a body drew: its label, its value and its generator's
/// span.
#[derive(Clone)]
pub(crate) struct Drawn {
    pub(crate) label: String,
    pub(crate) value: Rendered,
    pub(crate) span: usize,
    pub(crate) generator: Arc<dyn Erased>,
}

impl fmt::Debug for Drawn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Drawn")
            .field("label", &self.label)
            .field("value", &self.value)
            .field("span", &self.span)
            .finish_non_exhaustive()
    }
}

/// The failure of a case, with the identity by which the shrinker keeps it.
///
/// Two failures are one failure when their identities are equal. The message
/// is not part of the identity.
#[derive(Debug, Clone, PartialEq)]
pub struct Failed {
    /// The identity.
    pub identity: String,
    /// The record of the failure, which the report of a run states.
    pub(crate) record: Option<Box<Failure>>,
    /// The identity in the form of a store entry.
    pub(crate) entry: Vec<(&'static str, Value)>,
}

impl Failed {
    /// Returns the failure of an identity.
    #[must_use]
    pub fn of(identity: &str) -> Self {
        Self {
            identity: identity.to_owned(),
            record: None,
            entry: Vec::new(),
        }
    }
}

/// What ends a call of a body before it returns.
#[derive(Debug, Clone, PartialEq)]
pub enum Stop {
    /// The case states no input of the domain, and is not counted.
    Rejected,
    /// The case asked for more choices than its cap allows.
    Overrun,
    /// The case skipped the run of its property, with the reason.
    Skipped(String),
    /// The body failed.
    Failed(Box<Failed>),
    /// A draw met an entry of a trace that the body cannot follow.
    Trace(TraceError),
    /// A fault that ends the run, with its text.
    Fault(String),
    /// A fault that ended a call of the body. It ends the run in every phase,
    /// and the property ends its call with the same fault.
    Faulted(CaseFault),
}

/// The fault that ended a call of a body, which [`Stop::Faulted`] carries.
/// Two of them are equal when they are the same error.
#[derive(Debug, Clone)]
pub struct CaseFault(pub Arc<dyn std::error::Error + Send + Sync>);

impl PartialEq for CaseFault {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::addr_eq(Arc::as_ptr(&self.0), Arc::as_ptr(&other.0))
    }
}

/// The payload with which a case unwinds a body that it ends. The engine
/// catches it, and `resume_unwind` raises it without the panic hook.
pub(crate) struct Signal(pub(crate) Stop);

/// Returns a value as [`Any`], so a caller can read the provider of a case
/// as the kind that it made.
pub(crate) trait AsAny {
    fn as_any(&self) -> &dyn Any;
}

impl<T: Any> AsAny for T {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Where a case's values come from.
pub(crate) trait Provider: Send + AsAny {
    /// Returns the value for `request` at `index` of the case.
    fn value(&mut self, request: &Request, index: usize) -> Choice;

    /// Returns the next stated value of a draw, for a provider that states
    /// the values of the draws.
    fn valued(&mut self) -> Option<Box<dyn Any + Send>> {
        None
    }

    /// Returns the provider as a trace's provider.
    fn tracing(&mut self) -> Option<&mut Tracing> {
        None
    }
}

/// A provider that draws every value from a random source.
///
/// A request marked reuse, whose bounds admit more than one value and match
/// the bounds of an earlier reuse request of the same case, first takes
/// `coin(1, 4)`. When it comes up, the value is one of the earlier values with
/// those bounds, chosen with `below(their count)`. Otherwise the request draws
/// as usual. The earlier values are those of the choices that the record
/// still contains, so a filter's next attempt never reuses a rejected one.
///
/// The provider keeps the state of its source before the first value and
/// after each value, so that a run on more than one worker can resume the
/// source where a run on one worker stops a case.
#[derive(Debug)]
pub(crate) struct Generating {
    source: Source,
    earlier: HashMap<IntegerBounds, Vec<(usize, Choice)>>,
    states: Vec<Source>,
}

impl Generating {
    pub(crate) fn new(source: Source) -> Self {
        Self {
            source,
            earlier: HashMap::new(),
            states: vec![source],
        }
    }

    fn draw(&mut self, request: &Request, index: usize) -> Choice {
        for kept in self.earlier.values_mut() {
            while kept.last().is_some_and(|(at, _)| *at >= index) {
                kept.pop();
            }
        }
        // A reuse request is an integer request, and one whose bounds admit
        // one value has nothing to reuse.
        let reusable = match &request.bounds {
            Bounds::Integer(bounds) if request.reuse && bounds.lo() != bounds.hi() => Some(*bounds),
            _ => None,
        };
        let Some(bounds) = reusable else {
            return request.draw.run(&mut self.source, &request.bounds);
        };
        let earlier = self.earlier.entry(bounds).or_default();
        let value = if !earlier.is_empty() && self.source.coin(1, REUSE_ODDS) {
            let at = usize::try_from(self.source.below(super::draw::wide(earlier.len())))
                .unwrap_or_default();
            earlier[at].1.clone()
        } else {
            request.draw.run(&mut self.source, &request.bounds)
        };
        earlier.push((index, value.clone()));
        value
    }
}

impl Generating {
    /// Returns the source after the first `n` values, or after every value
    /// for `None`.
    pub(crate) fn after(&self, n: Option<usize>) -> Option<Source> {
        match n {
            Some(n) => self.states.get(n).copied(),
            None => self.states.last().copied(),
        }
    }
}

impl Provider for Generating {
    fn value(&mut self, request: &Request, index: usize) -> Choice {
        let value = self.draw(request, index);
        self.states.push(self.source);
        value
    }
}

/// A provider that reads recorded choices back, in order, and states the
/// values of a case's draws that an example of values states.
///
/// A recorded choice that does not fit the request is coerced by the
/// request's bounds. A request past the last recorded choice takes the
/// target of its bounds. A draw takes the next stated value without a choice,
/// and a draw past the last stated value decodes.
pub(crate) struct Replaying {
    choices: Vec<Choice>,
    values: std::vec::IntoIter<Box<dyn Any + Send>>,
}

impl Replaying {
    pub(crate) fn new(choices: Vec<Choice>) -> Self {
        Self::stating(choices, Vec::new())
    }

    /// Returns the provider of `choices` whose draws take `values` first.
    pub(crate) fn stating(choices: Vec<Choice>, values: Vec<Box<dyn Any + Send>>) -> Self {
        Self {
            choices,
            values: values.into_iter(),
        }
    }
}

impl Provider for Replaying {
    fn value(&mut self, request: &Request, index: usize) -> Choice {
        match self.choices.get(index) {
            Some(recorded) => request.bounds.coerce(recorded),
            None => request.bounds.target(),
        }
    }

    fn valued(&mut self) -> Option<Box<dyn Any + Send>> {
        self.values.next()
    }
}

/// The record of one call of a body.
///
/// `wheres` states where the case made each recorded request, and `observed`
/// where it observed each fingerprint. A machine's steps set `place` while
/// they run.
pub(crate) struct Record {
    provider: Box<dyn Provider>,
    max_choices: usize,
    cost: usize,
    open: Vec<usize>,
    drawing: Vec<String>,
    pub(crate) choices: Vec<Choice>,
    pub(crate) requests: Vec<Request>,
    pub(crate) spans: Vec<Span>,
    pub(crate) draws: Vec<Drawn>,
    pub(crate) steps: Vec<(usize, Step)>,
    pub(crate) labels: BTreeSet<String>,
    pub(crate) notes: Vec<String>,
    pub(crate) fingerprints: Vec<u64>,
    pub(crate) targets: BTreeMap<String, f64>,
    pub(crate) wheres: Vec<Where>,
    pub(crate) observed: Vec<Where>,
    pub(crate) place: Option<Place>,
    pub(crate) walk: Vec<Walked>,
    pub(crate) repeats: usize,
    /// The leaves of each recursive value in progress, by its generator.
    pub(crate) leaves: HashMap<usize, Vec<usize>>,
    /// The nodes of each value of a recursive shape in progress, by its
    /// budget.
    pub(crate) budgets: HashMap<usize, Vec<usize>>,
}

impl fmt::Debug for Record {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Record")
            .field("choices", &self.choices)
            .field("spans", &self.spans)
            .field("draws", &self.draws)
            .finish_non_exhaustive()
    }
}

impl Record {
    /// Returns an empty record whose values come from `provider`.
    pub(crate) fn new(provider: Box<dyn Provider>, max_choices: usize) -> Self {
        Self {
            provider,
            max_choices,
            cost: 0,
            open: Vec::new(),
            drawing: Vec::new(),
            choices: Vec::new(),
            requests: Vec::new(),
            spans: Vec::new(),
            draws: Vec::new(),
            steps: Vec::new(),
            labels: BTreeSet::new(),
            notes: Vec::new(),
            fingerprints: Vec::new(),
            targets: BTreeMap::new(),
            wheres: Vec::new(),
            observed: Vec::new(),
            place: None,
            walk: Vec::new(),
            repeats: 1,
            leaves: HashMap::new(),
            budgets: HashMap::new(),
        }
    }

    pub(crate) fn provider(&mut self) -> &mut dyn Provider {
        &mut *self.provider
    }

    /// Returns where the case is: the open draw's label and the machine's
    /// place.
    pub(crate) fn place_now(&self) -> Where {
        Where {
            label: self.drawing.last().cloned(),
            place: self.place.clone(),
        }
    }

    /// Returns the value for `request` and records it.
    ///
    /// A sequence whose minimum length alone takes the case past its cap
    /// ends the case before the provider makes its value, which would make
    /// that many elements.
    pub(crate) fn choose(&mut self, request: Request) -> Result<Choice, Stop> {
        if let Bounds::Sequence(bounds) = &request.bounds {
            if self.cost + 1 + bounds.min_size() > self.max_choices {
                return Err(Stop::Overrun);
            }
        }
        let index = self.choices.len();
        let value = self.provider.value(&request, index);
        self.cost += value.cost();
        if self.cost > self.max_choices {
            return Err(Stop::Overrun);
        }
        let place = self.place_now();
        self.walk.push(Walked {
            index,
            request: request.clone(),
            value: value.clone(),
            place: place.clone(),
        });
        self.choices.push(value.clone());
        self.requests.push(request);
        self.wheres.push(place);
        Ok(value)
    }

    /// Returns an integer choice inside `bounds`.
    pub(crate) fn integer(
        &mut self,
        bounds: IntegerBounds,
        edge: Option<i128>,
        reuse: bool,
    ) -> Result<i128, Stop> {
        let request = Request {
            bounds: Bounds::Integer(bounds),
            draw: Draw::Bounds,
            edge,
            reuse,
        };
        Ok(self.choose(request)?.as_integer().unwrap_or_default())
    }

    /// Returns a sequence choice inside `bounds`. Every provider's value of a
    /// sequence request is a sequence.
    pub(crate) fn sequence(&mut self, bounds: SequenceBounds) -> Result<Vec<u64>, Stop> {
        let chosen = self.choose(Request::of(Bounds::Sequence(bounds)))?;
        Ok(chosen.as_sequence().unwrap_or_default().to_vec())
    }

    /// Opens a span that starts at `start`, or at the current index, and
    /// returns its index.
    pub(crate) fn open(&mut self, label: &'static str, start: Option<usize>) -> usize {
        let parent = self.open.last().copied();
        let index = self.spans.len();
        let first = start.unwrap_or(self.choices.len());
        self.spans.push(Span {
            label,
            start: first,
            end: first,
            depth: self.open.len(),
            parent,
        });
        self.open.push(index);
        index
    }

    /// Closes the span at `index` at the current index.
    pub(crate) fn close(&mut self, index: usize) {
        if self.open.last() == Some(&index) {
            self.open.pop();
        }
        if let Some(span) = self.spans.get_mut(index) {
            span.end = self.choices.len();
        }
    }

    /// Returns the current point of the record, for a rewind.
    pub(crate) fn mark(&self) -> Mark {
        Mark {
            choices: self.choices.len(),
            spans: self.spans.len(),
            draws: self.draws.len(),
        }
    }

    /// Removes every choice, span and draw recorded after `mark`. The removed
    /// choices still count towards the cap, and the walk keeps them.
    pub(crate) fn rewind(&mut self, mark: Mark) {
        self.choices.truncate(mark.choices);
        self.requests.truncate(mark.choices);
        self.wheres.truncate(mark.choices);
        self.spans.truncate(mark.spans);
        self.draws.truncate(mark.draws);
    }

    /// Returns the choices of the record after the first `n` steps of the
    /// walk, as the record was before the rewinds of the later steps.
    pub(crate) fn record_at(&self, n: usize) -> Vec<Choice> {
        let mut record: Vec<Choice> = Vec::new();
        for step in self.walk.iter().take(n) {
            record.truncate(step.index);
            record.push(step.value.clone());
        }
        record
    }

    /// Marks the start of a draw labelled `label`, and returns its first
    /// span.
    pub(crate) fn begin_draw(&mut self, label: &str) -> usize {
        self.drawing.push(label.to_owned());
        self.spans.len()
    }

    /// Marks the end of the draw that [`Record::begin_draw`] started.
    pub(crate) fn end_draw(&mut self) {
        self.drawing.pop();
    }

    /// Records a step that a machine took, after every draw recorded so far.
    pub(crate) fn step(&mut self, taken: Step) {
        self.steps.push((self.draws.len(), taken));
    }

    /// Records a fingerprint of the subject's state, for a replay to compare.
    pub(crate) fn observe(&mut self, fingerprint: u64) {
        self.fingerprints.push(fingerprint);
        let place = self.place_now();
        self.observed.push(place);
    }

    /// Records a score under `label`, keeping the higher of two.
    pub(crate) fn target(&mut self, label: &str, score: f64) {
        let best = self.targets.entry(label.to_owned()).or_insert(score);
        if score > *best {
            *best = score;
        }
    }
}

/// The identity of a failure in the form that the shrinker compares, built
/// from its parts: the assertion and its call site, the assertion and its
/// contract, a message's call site, or a panic and its location.
pub(crate) fn identity_key(
    assertion: Option<&str>,
    contract: Option<&str>,
    error: Option<&str>,
    at: Option<CallSite>,
) -> String {
    let text =
        |part: Option<&str>| part.map_or_else(|| "null".to_owned(), |held| format!("{held:?}"));
    let (file, line) = at.map_or(("null".to_owned(), "null".to_owned()), |at| {
        (format!("{:?}", at.base_name()), at.line().to_string())
    });
    format!(
        "[{},{},{},{file},{line}]",
        text(assertion),
        text(contract),
        text(error)
    )
}
