// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The search of one partition for a linearization, which the definition
//! fixes: Wing and Gong's scan of the calls, with a memo of the
//! configurations that it visited.
//!
//! The entries are the invocations of the partition's calls and the
//! completions of its known calls, in event order. A call that is not known
//! has no completion entry: it takes effect at some point after its
//! invocation, or never. The candidates at each point are the calls whose
//! invocations come before the first completion of a call not yet
//! linearized, and an accepted call restarts the scan from the first entry.
//! The search passes when every known call is linearized.
//!
//! A configuration is a set of linearized calls and the states they leave,
//! no two of them equal. Two configurations are the same when their sets are
//! equal and their states are the same states in any order. The search stops
//! as undecided before a step that would take it past its budget, before it
//! stores a configuration that would take the memo past its limit, which
//! counts one bit per call of the partition for each configuration, and once
//! its time has passed.
//!
//! The frontier of a search is its first configuration, in search order,
//! with the most linearized calls. The search states the frontier's order,
//! its states and the candidates that the spec rejected there.

use super::partition::Checked;
use super::spec::{Operation, Spec};
use crate::matcher::panics::{message, quietly};
use std::collections::HashMap;
use std::time::Instant;

/// The sets of states that a search stored, by their configuration's
/// linearized calls and the sorted hashes of the states.
type Memo<S> = HashMap<(Vec<u64>, Vec<u64>), Vec<Vec<S>>>;

/// How a check or the search of a partition ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Verdict {
    Passed,
    Violated,
    Undecided,
}

impl Verdict {
    /// Returns the definition's spelling of the verdict, which the record of
    /// a check that did not pass states.
    pub(crate) fn name(self) -> &'static str {
        ["passed", "violated", "undecided"][self as usize]
    }
}

/// The limit that stopped an undecided search.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Limit {
    Steps,
    Memo,
    Time,
}

impl Limit {
    /// Returns the definition's spelling of the limit.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Steps => "steps",
            Self::Memo => "memo",
            Self::Time => "time",
        }
    }
}

/// The steps between two readings of the clock.
const CLOCK_STEPS: u64 = 1024;

/// How the search of one partition ended, and its frontier.
#[derive(Debug, Clone)]
pub(crate) struct Ending<S> {
    pub(crate) outcome: Verdict,
    pub(crate) steps: u64,
    /// The positions of the frontier's calls, in order, for a search that did
    /// not pass.
    pub(crate) linearized: Vec<usize>,
    /// The frontier's states, or the states after the order that a search
    /// that passed found.
    pub(crate) states: Vec<S>,
    /// The positions of the calls that the spec rejected at the frontier.
    pub(crate) candidates: Vec<usize>,
    pub(crate) limit: Option<Limit>,
}

/// A function of a spec that panicked during a search: its name, the call
/// that the search stepped, and the message of the panic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SpecPanic {
    pub(crate) function: &'static str,
    pub(crate) stepped: Option<(usize, String)>,
    pub(crate) message: String,
}

impl SpecPanic {
    /// Returns the place of the fault, `calls[n]` for the stepped call, and
    /// its reason.
    pub(crate) fn place(&self) -> (String, String) {
        match &self.stepped {
            Some((call, operation)) => (
                format!("calls[{call}]"),
                format!(
                    "the spec's {} panics on {operation:?}: {}",
                    self.function, self.message
                ),
            ),
            None => (
                String::new(),
                format!("the spec's {} panics: {}", self.function, self.message),
            ),
        }
    }
}

/// The search of one partition.
pub(crate) struct Search<'a, S: Spec + ?Sized> {
    calls: &'a [Checked],
    spec: &'a S,
    budget: u64,
    memo_limit: u64,
    deadline: Option<Instant>,
    clock_at: u64,
    /// The next and the previous entry of each entry, with the head and the
    /// tail after the entries.
    next: Vec<usize>,
    prev: Vec<usize>,
    head: usize,
    /// The position of the call of each entry, and whether the entry is an
    /// invocation.
    position: Vec<usize>,
    invokes: Vec<bool>,
    /// The entry of each call's invocation, and of a known call's completion.
    invocation: Vec<usize>,
    completion: Vec<Option<usize>>,
    /// The known calls that are not linearized.
    open: usize,
    steps: u64,
    /// The set of linearized calls, a bit per position.
    done: Vec<u64>,
    states: Vec<S::State>,
    stack: Vec<(usize, Vec<S::State>)>,
    memo: Memo<S::State>,
    configurations: u64,
    linearized: Vec<usize>,
    frontier: Vec<S::State>,
    candidates: Vec<usize>,
    at_frontier: bool,
    /// The spec's function that the search calls, and the position of the
    /// call that it steps.
    calling: &'static str,
    stepped: Option<usize>,
}

impl<'a, S: Spec + ?Sized> Search<'a, S> {
    /// Lays out the entries of a partition's calls in event order, before the
    /// first step.
    pub(crate) fn new(
        calls: &'a [Checked],
        spec: &'a S,
        budget: u64,
        memo_limit: u64,
        deadline: Option<Instant>,
    ) -> Self {
        let mut entries: Vec<(usize, usize, bool)> = Vec::new();
        for (position, Checked { span, .. }) in calls.iter().enumerate() {
            entries.push((span.call, position, true));
            if let (true, Some(completion)) = (span.operation.known(), span.completion) {
                entries.push((completion, position, false));
            }
        }
        entries.sort_unstable();
        let n = entries.len();
        let chain: Vec<usize> = std::iter::once(n)
            .chain(0..n)
            .chain(std::iter::once(n + 1))
            .collect();
        let (mut next, mut prev) = (vec![0; n + 2], vec![0; n + 2]);
        for pair in chain.windows(2) {
            next[pair[0]] = pair[1];
            prev[pair[1]] = pair[0];
        }
        let mut invocation = vec![0; calls.len()];
        let mut completion = vec![None; calls.len()];
        for (entry, &(_, position, invokes)) in entries.iter().enumerate() {
            if invokes {
                invocation[position] = entry;
            } else {
                completion[position] = Some(entry);
            }
        }
        Self {
            calls,
            spec,
            budget,
            memo_limit,
            deadline,
            clock_at: 0,
            next,
            prev,
            head: n,
            position: entries.iter().map(|&(_, position, _)| position).collect(),
            invokes: entries.iter().map(|&(_, _, invokes)| invokes).collect(),
            invocation,
            completion,
            open: calls
                .iter()
                .filter(|call| call.span.operation.known())
                .count(),
            steps: 0,
            done: vec![0; calls.len().div_ceil(64)],
            states: Vec::new(),
            stack: Vec::new(),
            memo: HashMap::new(),
            configurations: 0,
            linearized: Vec::new(),
            frontier: Vec::new(),
            candidates: Vec::new(),
            at_frontier: true,
            calling: "initial",
            stepped: None,
        }
    }

    /// Searches the partition, and returns how the search ended.
    ///
    /// # Errors
    ///
    /// Returns the panic of a function of the spec.
    pub(crate) fn run(mut self) -> Result<Ending<S::State>, SpecPanic> {
        match quietly(|| self.searched()) {
            Ok(ending) => Ok(ending),
            Err(payload) => Err(SpecPanic {
                function: self.calling,
                stepped: self.stepped.map(|position| {
                    let span = &self.calls[position].span;
                    (span.call, span.operation.name().to_owned())
                }),
                message: message(&*payload),
            }),
        }
    }

    /// Searches the partition from the spec's initial state.
    fn searched(&mut self) -> Ending<S::State> {
        self.states = vec![self.spec.initial()];
        self.frontier = self.states.clone();
        match self.search() {
            Ok(true) => Ending {
                outcome: Verdict::Passed,
                steps: self.steps,
                linearized: Vec::new(),
                states: self.states.clone(),
                candidates: Vec::new(),
                limit: None,
            },
            Ok(false) => self.ending(Verdict::Violated, None),
            Err(limit) => self.ending(Verdict::Undecided, Some(limit)),
        }
    }

    /// Returns an ending that states the frontier.
    fn ending(&self, outcome: Verdict, limit: Option<Limit>) -> Ending<S::State> {
        Ending {
            outcome,
            steps: self.steps,
            linearized: self.linearized.clone(),
            states: self.frontier.clone(),
            candidates: self.candidates.clone(),
            limit,
        }
    }

    /// Scans the entries until every known call is linearized, and returns
    /// whether one order passes.
    fn search(&mut self) -> Result<bool, Limit> {
        let mut entry = self.next[self.head];
        while self.open > 0 {
            if self.invokes.get(entry) != Some(&true) {
                let Some((position, states)) = self.stack.pop() else {
                    return Ok(false);
                };
                entry = self.backtrack(position, states);
            } else if self.linearize(self.position[entry])? {
                entry = self.next[self.head];
            } else {
                entry = self.next[entry];
            }
        }
        Ok(true)
    }

    /// Linearizes a candidate when the spec accepts it in a new
    /// configuration, and returns whether it did.
    fn linearize(&mut self, position: usize) -> Result<bool, Limit> {
        self.stepped = Some(position);
        let calls = self.calls;
        let following = self.step(&calls[position].span.operation)?;
        if following.is_empty() {
            if self.at_frontier {
                self.candidates.push(position);
            }
            return Ok(false);
        }
        let mut done = self.done.clone();
        done[position / 64] |= 1 << (position % 64);
        if !self.remember(&done, &following)? {
            return Ok(false);
        }
        let states = std::mem::replace(&mut self.states, following);
        self.stack.push((position, states));
        self.done = done;
        self.lift(position);
        self.at_frontier = self.stack.len() > self.linearized.len();
        if self.at_frontier {
            self.linearized = self.stack.iter().map(|(at, _)| *at).collect();
            self.frontier = self.states.clone();
            self.candidates.clear();
        }
        Ok(true)
    }

    /// Returns the states that `operation` leaves from the current ones,
    /// without two equal states.
    fn step(&mut self, operation: &Operation) -> Result<Vec<S::State>, Limit> {
        let mut following: Vec<S::State> = Vec::new();
        for state in &self.states {
            self.calling = "next";
            let cost = self.spec.cost(state);
            if self.steps.saturating_add(cost) > self.budget {
                return Err(Limit::Steps);
            }
            if let Some(deadline) = self.deadline {
                if self.steps >= self.clock_at {
                    if Instant::now() >= deadline {
                        return Err(Limit::Time);
                    }
                    self.clock_at = self.steps + CLOCK_STEPS;
                }
            }
            self.steps += cost;
            for after in self.spec.next(state, operation) {
                self.calling = "equal";
                if !following.iter().any(|kept| self.spec.equal(&after, kept)) {
                    following.push(after);
                }
                self.calling = "next";
            }
        }
        Ok(following)
    }

    /// Stores a configuration, and returns whether the memo lacked it.
    fn remember(&mut self, done: &[u64], states: &[S::State]) -> Result<bool, Limit> {
        self.calling = "hash";
        let mut hashes: Vec<u64> = states.iter().map(|state| self.spec.hash(state)).collect();
        hashes.sort_unstable();
        hashes.dedup();
        self.calling = "equal";
        let name = (done.to_vec(), hashes);
        if let Some(bucket) = self.memo.get(&name) {
            let seen = bucket.iter().any(|seen| {
                seen.len() == states.len()
                    && states
                        .iter()
                        .all(|state| seen.iter().any(|other| self.spec.equal(state, other)))
            });
            if seen {
                return Ok(false);
            }
        }
        let calls = u64::try_from(self.calls.len()).unwrap_or(u64::MAX);
        if (self.configurations + 1).saturating_mul(calls) > self.memo_limit {
            return Err(Limit::Memo);
        }
        self.memo.entry(name).or_default().push(states.to_vec());
        self.configurations += 1;
        Ok(true)
    }

    /// Undoes the linearized call at `position`, which the stack held with
    /// the states before it, and returns the entry after its invocation.
    fn backtrack(&mut self, position: usize, states: Vec<S::State>) -> usize {
        self.states = states;
        self.done[position / 64] &= !(1 << (position % 64));
        self.unlift(position);
        self.at_frontier = false;
        self.next[self.invocation[position]]
    }

    /// Takes a call's entries out of the scan.
    fn lift(&mut self, position: usize) {
        self.unlink(self.invocation[position]);
        if let Some(completion) = self.completion[position] {
            self.unlink(completion);
            self.open -= 1;
        }
    }

    /// Puts a call's entries back, in the reverse order of `lift`.
    fn unlift(&mut self, position: usize) {
        if let Some(completion) = self.completion[position] {
            self.relink(completion);
            self.open += 1;
        }
        self.relink(self.invocation[position]);
    }

    /// Takes one entry out of the list.
    fn unlink(&mut self, entry: usize) {
        let (before, after) = (self.prev[entry], self.next[entry]);
        self.next[before] = after;
        self.prev[after] = before;
    }

    /// Puts back an entry that `unlink` took out, between its old neighbours.
    fn relink(&mut self, entry: usize) {
        let (before, after) = (self.prev[entry], self.next[entry]);
        self.next[before] = entry;
        self.prev[after] = entry;
    }
}
