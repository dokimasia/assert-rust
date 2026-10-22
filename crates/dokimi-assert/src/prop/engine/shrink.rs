// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! Shrinking: the search for the smallest case that fails the same way.
//!
//! A case is smaller than another when its choice sequence is shorter, or
//! equally long and smaller in the first choice where the two differ. Choices
//! of one kind compare by their sort keys, and choices of different kinds by
//! kind: integer, then float, then sequence.
//!
//! Before shrinking, the failing case is replayed once from its choices. A
//! replay that requests another choice, observes another fingerprint, or ends
//! another way makes the run flaky.
//!
//! The shrinker runs its passes over the best case of one failure, in a fixed
//! order, until a whole round accepts no candidate. `delete-and-lower` runs
//! only in a round in which no other pass accepted a candidate. A candidate is
//! run only when it is smaller than the best case and no earlier candidate had
//! the same choices. It is accepted when its run fails with the same identity
//! and its recorded choices are smaller than the best case's. A run that
//! fails with another identity is kept as a further failure. Every failure is
//! shrunk in turn, the one with the smallest case first, and all of them share
//! one budget of runs.
//!
//! The explain phase then fills each draw of the counterexample with four
//! random values. A draw for which every filling still fails the same way is
//! one where any value fails. For an integer draw that matters, one step
//! towards its target that passes is its nearest passing value.

use super::case::{
    Drawn, Generating, MAX_CHOICES, Record, Rendered, Replaying, Request, Span, Where,
};
use super::choice::{
    Bounds, Choice, FloatBounds, INTEGRAL_LIMIT, IntegerBounds, Shortlex, fraction_bits,
    inverse_power_of_two, scaled, signed,
};
use super::draw::wide;
use super::execution::{Divergence, Execution, Fault, Phase, Status, Version};
use super::runner::{Run, Settings};
use super::source::Source;
use super::token;
use crate::matcher::panics::quietly;
use crate::prop::Case;
use std::cmp::Reverse;
use std::collections::{BTreeSet, HashMap};
use std::time::{Duration, Instant};

/// The runs of the body that shrinking every failure of a run may spend.
pub const DEFAULT_BUDGET: usize = 2000;

/// The random fillings that the explain phase tries for each draw.
const EXPLAIN_FILLINGS: u64 = 4;

/// The probes that `find_integer` makes one by one before it starts
/// doubling.
const LINEAR_PROBES: usize = 4;

/// The fewest siblings that `delete-span-chunk` deletes at once.
const SMALLEST_CHUNK: usize = 2;

/// One draw of the counterexample, explained.
///
/// `any_value_fails` is `None` when the draw makes no choice or the budget ran
/// out before its fillings. `nearest_passing` is the value one step towards
/// the target, for an integer draw where that value passes.
#[derive(Debug, Clone, PartialEq)]
pub struct Explained {
    /// The label of the draw.
    pub label: String,
    /// The value of the draw.
    pub value: Rendered,
    /// Whether every filling of the draw fails the same way.
    pub any_value_fails: Option<bool>,
    /// The nearest passing value of an integer draw.
    pub nearest_passing: Option<Rendered>,
}

/// What stops a shrink: a spent budget, or a fault of the run.
enum Halt {
    Exhausted,
    Fault(Fault),
}

impl From<Fault> for Halt {
    fn from(fault: Fault) -> Self {
        Self::Fault(fault)
    }
}

/// One recorded choice, with the request that it answered.
#[derive(Debug, Clone)]
struct Node {
    request: Request,
    choice: Choice,
}

impl Node {
    fn key(&self) -> super::choice::ChoiceKey {
        self.request.bounds.key(&self.choice)
    }

    /// Returns the node with its choice at its target.
    fn at_target(&self) -> Self {
        Self {
            request: self.request.clone(),
            choice: self.request.bounds.target(),
        }
    }

    /// Returns the elements of a sequence choice.
    fn elements(&self) -> &[u64] {
        self.choice.as_sequence().unwrap_or_default()
    }
}

/// Returns the shortlex key of a choice sequence.
fn key(nodes: &[Node]) -> Shortlex {
    Shortlex(nodes.iter().map(Node::key).collect())
}

/// Returns a case's choices with their requests.
fn nodes_of(record: &Record) -> Vec<Node> {
    record
        .requests
        .iter()
        .zip(&record.choices)
        .map(|(request, choice)| Node {
            request: request.clone(),
            choice: choice.clone(),
        })
        .collect()
}

/// Returns `nodes` with one choice replaced.
fn replace(nodes: &[Node], index: usize, choice: Choice) -> Vec<Node> {
    let mut changed = nodes.to_vec();
    changed[index].choice = choice;
    changed
}

/// Returns `nodes` without `[start, end)`.
fn without(nodes: &[Node], start: usize, end: usize) -> Vec<Node> {
    let mut kept = nodes[..start].to_vec();
    kept.extend_from_slice(&nodes[end..]);
    kept
}

/// The smallest case found so far that fails with one identity.
struct Failure {
    identity: String,
    execution: Execution,
    nodes: Vec<Node>,
    keys: Shortlex,
}

/// Returns a `k` of at most `limit` with `f(k)` true, given that `f(0)` is
/// true. `f(k + 1)` is false unless `k` is `limit`. It probes 1 to 4 in turn,
/// then doubles until `f` fails or the probe passes `limit`, then bisects. It
/// never calls `f` above `limit`.
fn find_integer(
    limit: usize,
    mut f: impl FnMut(usize) -> Result<bool, Halt>,
) -> Result<usize, Halt> {
    for k in 1..=LINEAR_PROBES {
        if k > limit || !f(k)? {
            return Ok(k - 1);
        }
    }
    let (mut low, mut high) = (LINEAR_PROBES, LINEAR_PROBES + 1);
    while high <= limit && f(high)? {
        low = high;
        high = high.saturating_mul(2);
    }
    high = high.min(limit.saturating_add(1));
    while low + 1 < high {
        let middle = usize::midpoint(low, high);
        if f(middle)? {
            low = middle;
        } else {
            high = middle;
        }
    }
    Ok(low)
}

/// Tries position 0, then bisects towards the smallest position that `at`
/// accepts. `position` is the current one, which counts as accepted. The
/// result reports whether any candidate was accepted.
fn search(position: u128, mut at: impl FnMut(u128) -> Result<bool, Halt>) -> Result<bool, Halt> {
    if position == 0 {
        return Ok(false);
    }
    if at(0)? {
        return Ok(true);
    }
    let (mut low, mut high, mut improved) = (0, position, false);
    while high - low > 1 {
        let middle = low + (high - low) / 2;
        if at(middle)? {
            high = middle;
            improved = true;
        } else {
            low = middle;
        }
    }
    Ok(improved)
}

/// Returns the seed of a draw's filling: `seed + (draw + 1) * 2^32 +
/// filling`, modulo 2^64.
fn explain_seed(seed: u64, draw: u64, filling: u64) -> u64 {
    seed.wrapping_add((draw + 1) << 32).wrapping_add(filling)
}

/// Replays a failing case once, and returns how the replay differed, if it
/// did.
///
/// The comparison takes the requests' bounds first, then the observed
/// fingerprints, then the way the replay ended.
pub(crate) fn confirm(
    run: &mut Run<'_, '_>,
    failing: &Execution,
    max_choices: usize,
) -> Result<Option<Divergence>, Fault> {
    let replay = run.execute(
        Box::new(Replaying::new(failing.record.choices.clone())),
        max_choices,
        None,
    )?;
    (run.observer)(Phase::Replay, &replay);
    let recorded: Vec<Version> = failing
        .record
        .requests
        .iter()
        .map(|request| Version::Bounds(request.bounds.clone()))
        .collect();
    let replayed: Vec<Version> = replay
        .record
        .requests
        .iter()
        .map(|request| Version::Bounds(request.bounds.clone()))
        .collect();
    if let Some(divergence) =
        first_difference("request", &recorded, &replayed, &replay.record.wheres)
    {
        return Ok(Some(divergence));
    }
    let fingerprints = |record: &Record| -> Vec<Version> {
        record
            .fingerprints
            .iter()
            .map(|fingerprint| Version::Fingerprint(*fingerprint))
            .collect()
    };
    if let Some(divergence) = first_difference(
        "fingerprint",
        &fingerprints(&failing.record),
        &fingerprints(&replay.record),
        &replay.record.observed,
    ) {
        return Ok(Some(divergence));
    }
    if replay.identity() != failing.identity() {
        let side = |identity: Option<&str>| {
            identity.map_or(Version::Nothing, |identity| {
                Version::Identity(identity.to_owned())
            })
        };
        return Ok(Some(Divergence {
            what: "verdict",
            index: failing.record.choices.len(),
            recorded: side(failing.identity()),
            replayed: side(replay.identity()),
            label: None,
            step: None,
        }));
    }
    Ok(None)
}

/// Returns the first position where two sides differ, with the place of the
/// replay's side there.
fn first_difference(
    what: &'static str,
    recorded: &[Version],
    replayed: &[Version],
    places: &[Where],
) -> Option<Divergence> {
    for index in 0..recorded.len().max(replayed.len()) {
        let before = recorded.get(index).cloned().unwrap_or(Version::Nothing);
        let after = replayed.get(index).cloned().unwrap_or(Version::Nothing);
        if before != after {
            let place = places.get(index).cloned().unwrap_or_default();
            return Some(Divergence::at(what, index, before, after, &place));
        }
    }
    None
}

/// The shrink of every failure of one run, over one budget.
pub(crate) struct Shrinker<'s, 'u, 'v> {
    run: &'s mut Run<'u, 'v>,
    failures: Vec<Failure>,
    target: String,
    budget: usize,
    runs: usize,
    deadline: Option<Instant>,
    max_choices: usize,
    sizes: HashMap<String, usize>,
}

/// A pass of the shrinker, which returns `true` when it accepted a
/// candidate.
type Pass<'s, 'u, 'v> = fn(&mut Shrinker<'s, 'u, 'v>) -> Result<bool, Halt>;

/// Passes each candidate that an enumeration lists to the shrinker, which
/// returns `true` when it accepted the candidate.
type Consider<'c> = &'c mut dyn FnMut(Vec<Node>) -> Result<bool, Halt>;

/// Lists the candidates of a pass for a case's nodes and spans, and returns
/// `true` when the shrinker accepted one.
type Enumerate = fn(&[Node], &[Span], Consider<'_>) -> Result<bool, Halt>;

impl<'s, 'u, 'v> Shrinker<'s, 'u, 'v> {
    /// Starts from the first failing case.
    pub(crate) fn new(run: &'s mut Run<'u, 'v>, first: Execution, settings: &Settings) -> Self {
        let target = first.identity().unwrap_or_default().to_owned();
        let mut sizes = HashMap::new();
        sizes.insert(
            token::encode(&first.record.choices),
            first.record.choices.len(),
        );
        let mut shrinker = Self {
            run,
            failures: Vec::new(),
            target,
            budget: settings.shrink,
            runs: 0,
            deadline: settings
                .shrink_time
                .filter(|limit| *limit > Duration::ZERO)
                .map(|limit| Instant::now() + limit),
            max_choices: settings.max_choices,
            sizes,
        };
        shrinker.record(first);
        shrinker
    }

    /// Returns the runs that shrinking and explaining spent.
    pub(crate) fn runs(&self) -> usize {
        self.runs
    }

    /// Returns the minimal case of `identity` and those of the other
    /// failures, in the order that they were found.
    pub(crate) fn into_failures(mut self, identity: &str) -> (Execution, Vec<Execution>) {
        // The shrinker keeps the first failure from the start.
        let first = self
            .failures
            .remove(self.position(identity).unwrap_or_default());
        let others = self
            .failures
            .into_iter()
            .map(|failure| failure.execution)
            .collect();
        (first.execution, others)
    }

    fn position(&self, identity: &str) -> Option<usize> {
        self.failures
            .iter()
            .position(|failure| failure.identity == identity)
    }

    fn best(&self) -> &Failure {
        let at = self.position(&self.target).unwrap_or_default();
        &self.failures[at]
    }

    fn nodes(&self) -> &[Node] {
        &self.best().nodes
    }

    fn spans(&self) -> &[Span] {
        &self.best().execution.record.spans
    }

    /// Keeps a failing run when it is its identity's first or smallest, and
    /// returns `true` when it became the best case.
    fn record(&mut self, execution: Execution) -> bool {
        let identity = execution.identity().unwrap_or_default().to_owned();
        let nodes = nodes_of(&execution.record);
        let keys = key(&nodes);
        let failure = Failure {
            identity: identity.clone(),
            execution,
            nodes,
            keys,
        };
        match self.position(&identity) {
            None => {
                self.failures.push(failure);
                false
            }
            Some(at) if failure.keys < self.failures[at].keys => {
                self.failures[at] = failure;
                identity == self.target
            }
            Some(_) => false,
        }
    }

    /// Runs the body on `choices` in `phase`, spending the runs of the
    /// budget that it makes.
    fn run(&mut self, choices: Vec<Choice>, phase: Phase) -> Result<Execution, Halt> {
        if self.runs >= self.budget
            || self
                .deadline
                .is_some_and(|deadline| Instant::now() > deadline)
        {
            return Err(Halt::Exhausted);
        }
        let room = self.budget - self.runs;
        let execution = self.run.execute(
            Box::new(Replaying::new(choices)),
            self.max_choices,
            Some(room),
        )?;
        self.runs += execution.calls.len().max(1);
        (self.run.observer)(phase, &execution);
        Ok(execution)
    }

    /// Runs a candidate that is smaller than the best case and new, and
    /// returns `true` when it became the best case.
    fn consider(&mut self, nodes: Vec<Node>) -> Result<bool, Halt> {
        if key(&nodes) >= self.best().keys {
            return Ok(false);
        }
        let choices: Vec<Choice> = nodes.into_iter().map(|node| node.choice).collect();
        let token = token::encode(&choices);
        if self.sizes.contains_key(&token) {
            return Ok(false);
        }
        let execution = self.run(choices, Phase::Shrink)?;
        self.sizes.insert(token, execution.record.choices.len());
        if execution.failure.is_none() {
            return Ok(false);
        }
        Ok(self.record(execution))
    }

    /// Shrinks each failure in turn, the one with the smallest case first,
    /// until every failure is done or the budget is spent.
    pub(crate) fn shrink_all(&mut self) -> Result<(), Fault> {
        let mut done: BTreeSet<String> = BTreeSet::new();
        loop {
            let next = self
                .failures
                .iter()
                .filter(|failure| !done.contains(&failure.identity))
                .min_by(|a, b| a.keys.cmp(&b.keys).then(a.identity.cmp(&b.identity)))
                .map(|failure| failure.identity.clone());
            let Some(identity) = next else {
                return Ok(());
            };
            match self.shrink(&identity) {
                Ok(()) => {
                    done.insert(identity);
                }
                Err(Halt::Exhausted) => return Ok(()),
                Err(Halt::Fault(fault)) => return Err(fault),
            }
        }
    }

    /// Runs rounds of every pass on one failure until a round accepts
    /// nothing.
    fn shrink(&mut self, identity: &str) -> Result<(), Halt> {
        identity.clone_into(&mut self.target);
        let passes: [Pass<'s, 'u, 'v>; 15] = [
            Self::delete_span_chunk,
            Self::delete_span,
            Self::lift_descendant,
            Self::delete_span_run,
            Self::sequence_delete,
            Self::delete_structure_pair,
            Self::target_span,
            Self::minimize_choice,
            Self::sequence_lower,
            Self::lower_and_delete,
            Self::sort_siblings,
            Self::redistribute,
            Self::lower_together,
            Self::minimize_duplicates,
            Self::float_simplify,
        ];
        loop {
            let mut improved = false;
            for pass in passes {
                improved |= pass(self)?;
            }
            if !improved && !self.delete_and_lower()? {
                return Ok(());
            }
        }
    }

    /// Tries the candidates that `enumerate` lists for the best case, in
    /// order, and starts over on the new best case after an acceptance.
    fn sweep(&mut self, enumerate: Enumerate) -> Result<bool, Halt> {
        let mut improved = false;
        loop {
            let nodes = self.nodes().to_vec();
            let spans = self.spans().to_vec();
            if !enumerate(&nodes, &spans, &mut |candidate| self.consider(candidate))? {
                return Ok(improved);
            }
            improved = true;
        }
    }

    /// Deletes `2^k` consecutive siblings, the largest `k` first and the last
    /// chunk first, down to two siblings.
    fn delete_span_chunk(&mut self) -> Result<bool, Halt> {
        self.sweep(|nodes, spans, consider| {
            for group in groups(spans) {
                let count = group.len();
                let mut size = largest_power_of_two(count);
                while size >= SMALLEST_CHUNK {
                    for first in (0..=count - size).rev() {
                        let (start, end) = (group[first].start, group[first + size - 1].end);
                        if start < end && consider(without(nodes, start, end))? {
                            return Ok(true);
                        }
                    }
                    size /= 2;
                }
            }
            Ok(false)
        })
    }

    /// Deletes one span, from the span that starts last to the first. Of two
    /// spans that start together, the longer is tried first.
    fn delete_span(&mut self) -> Result<bool, Halt> {
        self.sweep(|nodes, spans, consider| {
            let mut ordered: Vec<&Span> = spans.iter().collect();
            ordered.sort_by_key(|span| Reverse((span.start, span.end)));
            for span in ordered {
                if span.start < span.end && consider(without(nodes, span.start, span.end))? {
                    return Ok(true);
                }
            }
            Ok(false)
        })
    }

    /// Replaces a span by a shorter descendant with the same label.
    fn lift_descendant(&mut self) -> Result<bool, Halt> {
        self.sweep(|nodes, spans, consider| {
            for (index, span) in spans.iter().enumerate() {
                for inner in descendants(spans, index) {
                    let shorter = inner.end - inner.start < span.end - span.start;
                    if inner.label == span.label && shorter {
                        let mut candidate = nodes[..span.start].to_vec();
                        candidate.extend_from_slice(&nodes[inner.start..inner.end]);
                        candidate.extend_from_slice(&nodes[span.end..]);
                        if consider(candidate)? {
                            return Ok(true);
                        }
                    }
                }
            }
            Ok(false)
        })
    }

    /// Deletes as long a run of siblings as `find_integer` finds, at each
    /// start. After a deletion, the next start is the sibling after the one
    /// that ended the run.
    fn delete_span_run(&mut self) -> Result<bool, Halt> {
        let mut improved = false;
        for parent in parents(self.spans()) {
            let mut first = 0;
            loop {
                let group: Vec<Span> = children(self.spans(), parent);
                if first >= group.len() {
                    break;
                }
                let original = self.nodes().to_vec();
                let deleted = find_integer(group.len() - first, |count| {
                    let (start, end) = (group[first].start, group[first + count - 1].end);
                    self.consider(without(&original, start, end))
                })?;
                improved |= deleted > 0;
                first += 1;
            }
        }
        Ok(improved)
    }

    /// Deletes `2^k` consecutive elements of a sequence, the largest `k` first.
    /// Sequences are visited in order, and within one the last run first. No
    /// deletion goes below the sequence's minimum size.
    fn sequence_delete(&mut self) -> Result<bool, Halt> {
        self.sweep(|nodes, _, consider| {
            for (index, node) in nodes.iter().enumerate() {
                let Bounds::Sequence(bounds) = &node.request.bounds else {
                    continue;
                };
                let elements = node.elements();
                let count = elements.len();
                let mut size = largest_power_of_two(count);
                while size >= 1 {
                    if count - size >= bounds.min_size() {
                        for first in (0..=count - size).rev() {
                            let mut kept = elements[..first].to_vec();
                            kept.extend_from_slice(&elements[first + size..]);
                            if consider(replace(nodes, index, Choice::Sequence(kept)))? {
                                return Ok(true);
                            }
                        }
                    }
                    size /= 2;
                }
            }
            Ok(false)
        })
    }

    /// Deletes two adjacent free structure choices, from the last pair to the
    /// first, which joins two collections.
    fn delete_structure_pair(&mut self) -> Result<bool, Halt> {
        self.sweep(|nodes, _, consider| {
            for first in (0..nodes.len().saturating_sub(1)).rev() {
                if free_structure(&nodes[first])
                    && free_structure(&nodes[first + 1])
                    && consider(without(nodes, first, first + 2))?
                {
                    return Ok(true);
                }
            }
            Ok(false)
        })
    }

    /// Sets every choice of one span to its target, for each span in order.
    fn target_span(&mut self) -> Result<bool, Halt> {
        self.sweep(|nodes, spans, consider| {
            for span in spans {
                let mut candidate = nodes.to_vec();
                for node in &mut candidate[span.start..span.end] {
                    *node = node.at_target();
                }
                if consider(candidate)? {
                    return Ok(true);
                }
            }
            Ok(false)
        })
    }

    /// Moves integer choices of one value and of the integer bounds `bounds`
    /// together towards their target: the target first, then a binary search
    /// over the value's position in the key order of its bounds.
    fn lower(&mut self, indices: &[usize], bounds: IntegerBounds) -> Result<bool, Halt> {
        let nodes = self.nodes().to_vec();
        let value = nodes[indices[0]].choice.as_integer().unwrap_or_default();
        search(bounds.rank(value), |rank| {
            let mut moved = nodes.clone();
            for index in indices {
                moved[*index].choice = Choice::Integer(bounds.at_rank(rank));
            }
            self.consider(moved)
        })
    }

    /// Moves each choice towards its target, in order: an integer by its
    /// target and then a binary search, a float or a sequence by its target.
    fn minimize_choice(&mut self) -> Result<bool, Halt> {
        let mut improved = false;
        let mut index = 0;
        while index < self.nodes().len() {
            let node = self.nodes()[index].clone();
            improved |= if let Some(bounds) = node.request.bounds.as_integer() {
                self.lower(&[index], *bounds)?
            } else {
                let candidate = replace(self.nodes(), index, node.request.bounds.target());
                self.consider(candidate)?
            };
            index += 1;
        }
        Ok(improved)
    }

    /// Moves each element of each sequence towards 0: 0, then a binary
    /// search.
    fn sequence_lower(&mut self) -> Result<bool, Halt> {
        let mut improved = false;
        let mut index = 0;
        while index < self.nodes().len() {
            let mut position = 0;
            while index < self.nodes().len() && position < self.nodes()[index].elements().len() {
                improved |= self.lower_element(index, position)?;
                position += 1;
            }
            index += 1;
        }
        Ok(improved)
    }

    fn lower_element(&mut self, index: usize, position: usize) -> Result<bool, Halt> {
        let nodes = self.nodes().to_vec();
        let elements = nodes[index].elements().to_vec();
        let value = elements[position];
        search(u128::from(value), |element| {
            let mut changed = elements.clone();
            changed[position] = u64::try_from(element).unwrap_or_default();
            self.consider(replace(&nodes, index, Choice::Sequence(changed)))
        })
    }

    /// Steps each integer towards its target, deleting a later span that it
    /// sizes. A step accepted with a deletion is tried again on the same
    /// integer, so a count falls one element at a time.
    fn lower_and_delete(&mut self) -> Result<bool, Halt> {
        let mut improved = false;
        let mut index = 0;
        while index < self.nodes().len() {
            let before = self.best().keys.clone();
            let deleted = self.step_and_delete(index)?;
            improved |= self.best().keys != before;
            if !deleted {
                index += 1;
            }
        }
        Ok(improved)
    }

    /// Steps the integer at `index` by one, alone and then with a later span
    /// deleted, and returns `true` when a step with a deletion was accepted.
    fn step_and_delete(&mut self, index: usize) -> Result<bool, Halt> {
        let nodes = self.nodes().to_vec();
        let (Some(bounds), Some(value)) = (
            nodes[index].request.bounds.as_integer().copied(),
            nodes[index].choice.as_integer(),
        ) else {
            return Ok(false);
        };
        let target = bounds.target();
        if value == target {
            return Ok(false);
        }
        let step = if value > target { value - 1 } else { value + 1 };
        let stepped = replace(&nodes, index, Choice::Integer(step));
        if self.consider(stepped.clone())? {
            return Ok(false);
        }
        let choices: Vec<Choice> = stepped.iter().map(|node| node.choice.clone()).collect();
        let size = self.sizes.get(&token::encode(&choices)).copied();
        if size.is_none_or(|size| size == nodes.len()) {
            return Ok(false);
        }
        let mut later: Vec<Span> = self
            .spans()
            .iter()
            .filter(|span| index < span.start && span.start < span.end)
            .cloned()
            .collect();
        later.sort_by_key(|span| Reverse((span.start, span.end)));
        for span in later {
            if self.consider(without(&stepped, span.start, span.end))? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Steps each integer towards its target with earlier data deleted: one
    /// span that ends at or before the integer removed, or one element of an
    /// earlier sequence removed.
    fn delete_and_lower(&mut self) -> Result<bool, Halt> {
        self.sweep(|nodes, spans, consider| {
            for (index, node) in nodes.iter().enumerate() {
                let (Some(bounds), Some(value)) =
                    (node.request.bounds.as_integer(), node.choice.as_integer())
                else {
                    continue;
                };
                let target = bounds.target();
                if value == target {
                    continue;
                }
                let step = if value > target { value - 1 } else { value + 1 };
                let stepped = replace(nodes, index, Choice::Integer(step));
                let mut earlier: Vec<&Span> = spans
                    .iter()
                    .filter(|span| span.start < span.end && span.end <= index)
                    .collect();
                earlier.sort_by_key(|span| Reverse((span.start, span.end)));
                for span in earlier {
                    if consider(without(&stepped, span.start, span.end))? {
                        return Ok(true);
                    }
                }
                for at in (0..index).rev() {
                    let Bounds::Sequence(sequence) = &stepped[at].request.bounds else {
                        continue;
                    };
                    let elements = stepped[at].elements();
                    if elements.len() <= sequence.min_size() {
                        continue;
                    }
                    for position in (0..elements.len()).rev() {
                        let mut kept = elements[..position].to_vec();
                        kept.extend_from_slice(&elements[position + 1..]);
                        if consider(replace(&stepped, at, Choice::Sequence(kept)))? {
                            return Ok(true);
                        }
                    }
                }
            }
            Ok(false)
        })
    }

    /// Swaps adjacent siblings with one label when the later one is smaller.
    fn sort_siblings(&mut self) -> Result<bool, Halt> {
        self.sweep(|nodes, spans, consider| {
            for group in groups(spans) {
                for pair in group.windows(2) {
                    let (left, right) = (&pair[0], &pair[1]);
                    let first = &nodes[left.start..left.end];
                    let second = &nodes[right.start..right.end];
                    if left.label == right.label && key(second) < key(first) {
                        let mut candidate = nodes[..left.start].to_vec();
                        candidate.extend_from_slice(second);
                        candidate.extend_from_slice(&nodes[left.end..right.start]);
                        candidate.extend_from_slice(first);
                        candidate.extend_from_slice(&nodes[right.end..]);
                        if consider(candidate)? {
                            return Ok(true);
                        }
                    }
                }
            }
            Ok(false)
        })
    }

    /// Returns the next integer after `index` with the same bounds.
    fn later_with(&self, index: usize, bounds: &IntegerBounds) -> Option<usize> {
        let nodes = self.nodes();
        (index + 1..nodes.len())
            .find(|later| nodes[*later].request.bounds.as_integer() == Some(bounds))
    }

    /// Moves value from an integer to the next integer with the same bounds:
    /// the whole distance to its target, then a binary search for the largest
    /// amount that the property accepts. The sum of the two stays the same.
    fn redistribute(&mut self) -> Result<bool, Halt> {
        let mut improved = false;
        let mut index = 0;
        while index < self.nodes().len() {
            improved |= self.shift(index)?;
            index += 1;
        }
        Ok(improved)
    }

    fn shift(&mut self, index: usize) -> Result<bool, Halt> {
        let nodes = self.nodes().to_vec();
        let (Some(bounds), Some(value)) = (
            nodes[index].request.bounds.as_integer().copied(),
            nodes[index].choice.as_integer(),
        ) else {
            return Ok(false);
        };
        let target = bounds.target();
        let Some(later) = self.later_with(index, &bounds) else {
            return Ok(false);
        };
        if value == target {
            return Ok(false);
        }
        let other = nodes[later].choice.as_integer().unwrap_or_default();
        let sign: i128 = if value > target { 1 } else { -1 };
        let mut moved = |amount: i128| -> Result<bool, Halt> {
            let raised = other + sign * amount;
            if !bounds.admits(raised) {
                return Ok(false);
            }
            let lowered = replace(&nodes, index, Choice::Integer(value - sign * amount));
            self.consider(replace(&lowered, later, Choice::Integer(raised)))
        };
        let distance = signed(value.abs_diff(target));
        if moved(distance)? {
            return Ok(true);
        }
        let (mut low, mut high, mut improved) = (0, distance, false);
        while high - low > 1 {
            let middle = low + (high - low) / 2;
            if moved(middle)? {
                low = middle;
                improved = true;
            } else {
                high = middle;
            }
        }
        Ok(improved)
    }

    /// Moves an integer and the next integer with its bounds towards their
    /// targets by one amount, which keeps their difference. Both must lie on
    /// one side of the target.
    fn lower_together(&mut self) -> Result<bool, Halt> {
        let mut improved = false;
        let mut index = 0;
        while index < self.nodes().len() {
            improved |= self.lower_pair(index)?;
            index += 1;
        }
        Ok(improved)
    }

    fn lower_pair(&mut self, index: usize) -> Result<bool, Halt> {
        let nodes = self.nodes().to_vec();
        let (Some(bounds), Some(value)) = (
            nodes[index].request.bounds.as_integer().copied(),
            nodes[index].choice.as_integer(),
        ) else {
            return Ok(false);
        };
        let Some(later) = self.later_with(index, &bounds) else {
            return Ok(false);
        };
        let other = nodes[later].choice.as_integer().unwrap_or_default();
        let target = bounds.target();
        if (value > target) != (other > target) || value == target || other == target {
            return Ok(false);
        }
        let step: i128 = if value > target { -1 } else { 1 };
        let room = value.abs_diff(target).min(other.abs_diff(target));
        let found = find_integer(usize::try_from(room).unwrap_or(usize::MAX), |amount| {
            let amount = signed(wide(amount));
            let lowered = replace(&nodes, index, Choice::Integer(value + step * amount));
            self.consider(replace(
                &lowered,
                later,
                Choice::Integer(other + step * amount),
            ))
        })?;
        Ok(found > 0)
    }

    /// Moves every choice that shares a value and bounds with another towards
    /// the target together. Integers search the distance; floats and sequences
    /// try the target.
    fn minimize_duplicates(&mut self) -> Result<bool, Halt> {
        let mut improved = false;
        let mut group = 0;
        loop {
            let groups = duplicates(self.nodes());
            let Some(indices) = groups.get(group).cloned() else {
                return Ok(improved);
            };
            let bounds = self.nodes()[indices[0]]
                .request
                .bounds
                .as_integer()
                .copied();
            improved |= if let Some(bounds) = bounds {
                self.lower(&indices, bounds)?
            } else {
                let mut at_target = self.nodes().to_vec();
                for index in &indices {
                    at_target[*index] = at_target[*index].at_target();
                }
                self.consider(at_target)?
            };
            group += 1;
        }
    }

    /// Simplifies each float, in order: fewer fractional bits, then a smaller
    /// integer.
    fn float_simplify(&mut self) -> Result<bool, Halt> {
        let mut improved = false;
        let mut index = 0;
        while index < self.nodes().len() {
            let node = self.nodes()[index].clone();
            if let (Bounds::Float(bounds), Some(value)) =
                (&node.request.bounds, node.choice.as_float())
            {
                for rounded in rounded_floats(value, bounds) {
                    if self.consider(replace(self.nodes(), index, Choice::Float(rounded)))? {
                        improved = true;
                        break;
                    }
                }
                improved |= self.lower_float(index, *bounds)?;
            }
            index += 1;
        }
        Ok(improved)
    }

    /// Moves the integral float at `index`, of the bounds `bounds`, towards an
    /// integral target, as an integer, over the integers of the bounds below
    /// 2^53 in magnitude.
    fn lower_float(&mut self, index: usize, bounds: FloatBounds) -> Result<bool, Halt> {
        let nodes = self.nodes().to_vec();
        let value = nodes[index].choice.as_float().unwrap_or_default();
        let target = bounds.target();
        let integral = value.fract() == 0.0
            && target.fract() == 0.0
            && value.abs() < INTEGRAL_LIMIT
            && target.abs() < INTEGRAL_LIMIT;
        if !integral {
            return Ok(false);
        }
        let limit: i128 = (1 << 53) - 1;
        let lo = if bounds.lo().is_infinite() {
            -limit
        } else {
            whole(bounds.lo().ceil()).max(-limit)
        };
        let hi = if bounds.hi().is_infinite() {
            limit
        } else {
            whole(bounds.hi().floor()).min(limit)
        };
        let integers = IntegerBounds::of(lo, hi);
        search(integers.rank(whole(value)), |rank| {
            let candidate = float_of(integers.at_rank(rank));
            if !bounds.admits(candidate) {
                return Ok(false);
            }
            self.consider(replace(&nodes, index, Choice::Float(candidate)))
        })
    }
}

/// Returns an integral float below 2^53 in magnitude as an integer.
#[expect(
    clippy::cast_possible_truncation,
    reason = "the float is integral and below 2^53 in magnitude, so the integer is exact"
)]
fn whole(x: f64) -> i128 {
    x as i128
}

/// Returns an integer below 2^53 in magnitude as a float.
#[expect(
    clippy::cast_precision_loss,
    reason = "the integer is below 2^53 in magnitude, which a float states exactly"
)]
fn float_of(x: i128) -> f64 {
    x as f64
}

/// Returns the largest power of two that is `count` or less, and 0 for a
/// `count` of 0.
fn largest_power_of_two(count: usize) -> usize {
    count.checked_ilog2().map_or(0, |log| 1 << log)
}

/// Returns `true` when a choice decides structure and allows two values or
/// more.
fn free_structure(node: &Node) -> bool {
    node.request.edge.is_some()
        && node
            .request
            .bounds
            .as_integer()
            .is_some_and(|bounds| bounds.lo() < bounds.hi())
}

/// Returns the groups of choices that share bounds and a value off the
/// target, by their first choice.
fn duplicates(nodes: &[Node]) -> Vec<Vec<usize>> {
    let mut shared: Vec<((Bounds, String), Vec<usize>)> = Vec::new();
    for (index, node) in nodes.iter().enumerate() {
        if node.key() == node.at_target().key() {
            continue;
        }
        let identity = (
            node.request.bounds.clone(),
            token::encode(std::slice::from_ref(&node.choice)),
        );
        match shared.iter_mut().find(|(held, _)| *held == identity) {
            Some((_, indices)) => indices.push(index),
            None => shared.push((identity, vec![index])),
        }
    }
    shared
        .into_iter()
        .map(|(_, indices)| indices)
        .filter(|indices| indices.len() > 1)
        .collect()
}

/// Returns the spans whose parent is `parent`, in order.
fn children(spans: &[Span], parent: Option<usize>) -> Vec<Span> {
    spans
        .iter()
        .filter(|span| span.parent == parent)
        .cloned()
        .collect()
}

/// Returns every parent of a sibling group: the later parents first, then
/// the top.
fn parents(spans: &[Span]) -> Vec<Option<usize>> {
    let mut parents: Vec<usize> = spans.iter().filter_map(|span| span.parent).collect();
    parents.sort_unstable();
    parents.dedup();
    let mut ordered: Vec<Option<usize>> = parents.into_iter().rev().map(Some).collect();
    if !spans.is_empty() {
        ordered.push(None);
    }
    ordered
}

/// Returns the sibling groups, in the order of their parents.
fn groups(spans: &[Span]) -> Vec<Vec<Span>> {
    parents(spans)
        .into_iter()
        .map(|parent| children(spans, parent))
        .collect()
}

/// Returns the descendants of the span at `index`, in order.
fn descendants(spans: &[Span], index: usize) -> Vec<&Span> {
    let mut inside = vec![index];
    let mut found = Vec::new();
    for (later, span) in spans.iter().enumerate().skip(index + 1) {
        match span.parent {
            Some(parent) if inside.contains(&parent) => {
                inside.push(later);
                found.push(span);
            }
            _ => break,
        }
    }
    found
}

/// Returns a fraction rounded to fewer fractional bits, from 0 bits up: at
/// each number of bits the value rounded towards the target, then away from
/// it, where the bounds admit them. An integral value, an infinity and NaN
/// have none.
fn rounded_floats(value: f64, bounds: &FloatBounds) -> Vec<f64> {
    if value.is_nan() || value.is_infinite() {
        return Vec::new();
    }
    // The value is m / 2^k in lowest terms, so the value at f fractional bits
    // is m / 2^(k - f), with the sign of the value. A quotient of 1 or more is
    // exact, and a smaller one rounds to 0 or to a unit either way.
    let magnitude = value.abs();
    let k = fraction_bits(magnitude);
    let m = scaled(magnitude, k);
    let downward = value > bounds.target();
    let mut rounded = Vec::new();
    for f in 0..k {
        let at = (m * inverse_power_of_two(k - f)).copysign(value);
        let roundings: [fn(f64) -> f64; 2] = if downward {
            [f64::floor, f64::ceil]
        } else {
            [f64::ceil, f64::floor]
        };
        for rounding in roundings {
            // An integer of 0 has no sign, so the candidate is +0.
            let candidate = rounding(at) * inverse_power_of_two(f) + 0.0;
            if bounds.admits(candidate) {
                rounded.push(candidate);
            }
        }
    }
    rounded
}

/// Explains each draw of the counterexample of `identity`, spending what
/// remains of the budget.
pub(crate) fn explain(
    shrinker: &mut Shrinker<'_, '_, '_>,
    identity: &str,
    seed: u64,
) -> Result<Vec<Explained>, Fault> {
    identity.clone_into(&mut shrinker.target);
    let (draws, spans, nodes) = {
        let best = shrinker.best();
        (
            best.execution.record.draws.clone(),
            best.execution.record.spans.clone(),
            best.nodes.clone(),
        )
    };
    let mut explained = Vec::with_capacity(draws.len());
    for (number, drawn) in draws.iter().enumerate() {
        let empty = Explained {
            label: drawn.label.clone(),
            value: drawn.value.clone(),
            any_value_fails: None,
            nearest_passing: None,
        };
        let Some(span) = spans.get(drawn.span).filter(|span| span.start < span.end) else {
            explained.push(empty);
            continue;
        };
        let draw = u64::try_from(number).unwrap_or_default();
        let seeds: Vec<u64> = (0..EXPLAIN_FILLINGS)
            .map(|filling| explain_seed(seed, draw, filling))
            .collect();
        let relevance = relevance(shrinker, identity, &nodes, span, drawn, &seeds);
        let any_value = match relevance {
            Ok(any_value) => any_value,
            Err(Halt::Exhausted) => {
                explained.push(empty);
                continue;
            }
            Err(Halt::Fault(fault)) => return Err(fault),
        };
        let nearest = if any_value == Some(false) {
            match nearest(shrinker, &nodes, span, drawn) {
                Ok(nearest) => nearest,
                Err(Halt::Exhausted) => {
                    explained.push(empty);
                    continue;
                }
                Err(Halt::Fault(fault)) => return Err(fault),
            }
        } else {
            None
        };
        explained.push(Explained {
            any_value_fails: any_value,
            nearest_passing: nearest,
            ..empty
        });
    }
    Ok(explained)
}

/// Runs the fillings of one draw, and reports whether any value fails: `true`
/// when every filling that decoded fails the same way, `false` at the first
/// that does not, and `None` when none decoded.
fn relevance(
    shrinker: &mut Shrinker<'_, '_, '_>,
    identity: &str,
    nodes: &[Node],
    span: &Span,
    drawn: &Drawn,
    seeds: &[u64],
) -> Result<Option<bool>, Halt> {
    let mut decoded = false;
    for seed in seeds {
        let Some(choices) = filled(nodes, span, drawn, Source::new(*seed)) else {
            continue;
        };
        decoded = true;
        let execution = shrinker.run(choices, Phase::Explain)?;
        if execution.identity() != Some(identity) {
            return Ok(Some(false));
        }
    }
    Ok(decoded.then_some(true))
}

/// Returns the choices with one draw's span replaced by a filling from
/// `source`, or `None` when a fresh decode returns no value.
fn filled(nodes: &[Node], span: &Span, drawn: &Drawn, source: Source) -> Option<Vec<Choice>> {
    let fresh = Case::new(
        Record::new(Box::new(Generating::new(source)), MAX_CHOICES),
        None,
    );
    let decoded = quietly(|| drawn.generator.fill(&fresh));
    if !matches!(decoded, Ok(Ok(()))) {
        return None;
    }
    let (record, _) = fresh.into_parts();
    let mut choices: Vec<Choice> = nodes[..span.start]
        .iter()
        .map(|node| node.choice.clone())
        .collect();
    choices.extend(record.choices);
    choices.extend(nodes[span.end..].iter().map(|node| node.choice.clone()));
    Some(choices)
}

/// Returns an integer draw's value one step towards its target, when that
/// value passes.
fn nearest(
    shrinker: &mut Shrinker<'_, '_, '_>,
    nodes: &[Node],
    span: &Span,
    drawn: &Drawn,
) -> Result<Option<Rendered>, Halt> {
    let generator = &drawn.generator;
    let (Some(bounds), Some(value)) = (generator.integer(), nodes[span.start].choice.as_integer())
    else {
        return Ok(None);
    };
    let target = bounds.target();
    if value == target {
        return Ok(None);
    }
    let stepped = if value > target { value - 1 } else { value + 1 };
    let choices = replace(nodes, span.start, Choice::Integer(stepped))
        .into_iter()
        .map(|node| node.choice)
        .collect();
    if shrinker.run(choices, Phase::Explain)?.status() != Status::Passed {
        return Ok(None);
    }
    Ok(generator.nearest(stepped))
}
