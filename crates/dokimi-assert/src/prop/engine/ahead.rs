// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The executor of a run's generated cases, on one worker or on more.
//!
//! On one worker, each case runs on the thread of the runner when the runner
//! calls for it. On more, the workers run the random and the edge cases
//! ahead of the runner, in the order of one worker: random case 0, edge case
//! 0, random case 1, edge case 1, and so on, with the random cases alone after
//! the fourth edge case. A worker starts a case at most `workers` positions
//! past the furthest case that the runner called for. A call for a case
//! waits until the case has ended. A replayed case, the simplest case or a
//! prefix case, runs on the next free worker before any case ahead.
//!
//! Every case runs outside the case tree, and the runner enters it into the
//! tree when it takes it, so a run on any number of workers reports the same
//! outcome as a run on one.

use super::case::{AsAny, Generating, Provider, Replaying};
use super::choice::Choice;
use super::edge::BOUNDARIES;
use super::execution::{Body, Execution, Fault, Status, entered, execute};
use super::runner::{Run, Settings, edge, generating};
use super::source::Source;
use super::tree::Tree;
use crate::prop::case::Context;
use std::collections::{HashMap, VecDeque};
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::Scope;

/// A random case as the runner takes it: its execution in the tree, its
/// choices where a run on one worker stops it, and the source there, from
/// which the cut of its prefix case is drawn.
pub(crate) type Generated = (Execution, Vec<Choice>, Source);

/// Returns the position of random case `index` in the order of one worker.
fn random_position(index: u64) -> usize {
    let index = usize::try_from(index).unwrap_or(usize::MAX);
    (2 * index).min(index + BOUNDARIES.len())
}

/// Returns the index of the random case at `position`, or `None` for an edge
/// case.
fn random_at(position: usize) -> Option<u64> {
    if position % 2 == 1 && position / 2 < BOUNDARIES.len() {
        return None;
    }
    u64::try_from((position / 2).max(position.saturating_sub(BOUNDARIES.len()))).ok()
}

/// What runs the generated cases of a run.
pub(crate) struct Executor<'e, 's> {
    body: Body<'e, 's>,
    context: Option<&'s Context<'s>>,
    seed: u64,
    max_choices: usize,
    ahead: Option<&'e Ahead>,
}

impl<'s> Executor<'_, 's> {
    /// Runs `f` with the executor of a run's settings: one worker, or the
    /// workers of a scope that ends when `f` returns.
    pub(crate) fn with<R>(
        run: &mut Run<'_, 's>,
        settings: &Settings,
        f: impl FnOnce(&mut Run<'_, 's>, &mut Executor<'_, 's>) -> R,
    ) -> R {
        let body = run.body;
        let context = run.context;
        if settings.workers <= 1 {
            let mut executor = Executor {
                body,
                context,
                seed: settings.seed,
                max_choices: settings.max_choices,
                ahead: None,
            };
            return f(run, &mut executor);
        }
        let ahead = Ahead::new(settings.workers);
        std::thread::scope(|scope| {
            let ahead = &ahead;
            ahead.start(scope, body, context, settings.seed, settings.max_choices);
            let mut executor = Executor {
                body,
                context,
                seed: settings.seed,
                max_choices: settings.max_choices,
                ahead: Some(ahead),
            };
            let result = f(run, &mut executor);
            ahead.close();
            result
        })
    }

    /// Runs the case that replays `choices`, and enters it into the tree.
    pub(crate) fn replay(
        &mut self,
        tree: &mut Tree,
        choices: Vec<Choice>,
    ) -> Result<Execution, Fault> {
        let execution = match self.ahead {
            Some(ahead) => ahead.replay(choices)?,
            None => execute(
                self.body,
                Box::new(Replaying::new(choices)),
                self.max_choices,
                self.context,
                None,
            )?,
        };
        Ok(entered(tree, execution).0)
    }

    /// Takes random case `index` of the seed, and enters it into the tree.
    pub(crate) fn random(&mut self, tree: &mut Tree, index: u64) -> Result<Generated, Fault> {
        let execution = match self.ahead {
            Some(ahead) => ahead.take(random_position(index))?,
            None => execute(
                self.body,
                generating(self.seed, index),
                self.max_choices,
                self.context,
                None,
            )?,
        };
        let (mut execution, steps) = entered(tree, execution);
        let stopped = execution.status == Status::Repeated;
        let choices = if stopped {
            execution.record.record_at(steps)
        } else {
            execution.record.choices.clone()
        };
        // A random case reads its values from the generating provider of its
        // source, which keeps the source after each value.
        let provider: &dyn Provider = execution.record.provider();
        let source = AsAny::as_any(provider)
            .downcast_ref::<Generating>()
            .and_then(|generating| generating.after(stopped.then_some(steps)))
            .unwrap_or(super::source::case_source(self.seed, index));
        Ok((execution, choices, source))
    }

    /// Takes the edge case of boundary `number`, and enters it into the
    /// tree.
    pub(crate) fn edge(&mut self, tree: &mut Tree, number: usize) -> Result<Execution, Fault> {
        let execution = match self.ahead {
            Some(ahead) => ahead.take(2 * number + 1)?,
            None => execute(
                self.body,
                edge(number),
                self.max_choices,
                self.context,
                None,
            )?,
        };
        Ok(entered(tree, execution).0)
    }
}

/// The state of the workers of a run on more than one worker.
struct State {
    workers: usize,
    next: usize,
    taken: usize,
    replays: VecDeque<(usize, Vec<Choice>)>,
    replayed: usize,
    done: HashMap<Slot, Result<Execution, Fault>>,
    closed: bool,
}

/// A case of the run: a position of the order of one worker, or a replay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Slot {
    Position(usize),
    Replay(usize),
}

/// The workers of a run on more than one worker.
struct Ahead {
    state: Mutex<State>,
    changed: Condvar,
}

impl Ahead {
    fn new(workers: usize) -> Self {
        Self {
            state: Mutex::new(State {
                workers,
                next: 0,
                taken: 0,
                replays: VecDeque::new(),
                replayed: 0,
                done: HashMap::new(),
                closed: false,
            }),
            changed: Condvar::new(),
        }
    }

    fn locked(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn wait<'g>(&self, guard: MutexGuard<'g, State>) -> MutexGuard<'g, State> {
        self.changed
            .wait(guard)
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Starts the workers on `scope`.
    fn start<'scope, 's>(
        &'scope self,
        scope: &'scope Scope<'scope, '_>,
        body: Body<'scope, 's>,
        context: Option<&'s Context<'s>>,
        seed: u64,
        max_choices: usize,
    ) where
        's: 'scope,
    {
        let workers = self.locked().workers;
        for _ in 0..workers {
            scope.spawn(move || {
                while let Some((slot, provider)) = self.claim(seed) {
                    let result = execute(body, provider, max_choices, context, None);
                    self.locked().done.insert(slot, result);
                    self.changed.notify_all();
                }
            });
        }
    }

    /// Returns the next case for a worker, or `None` once the run has
    /// closed: a replay first, then the next case ahead while it is at most
    /// `workers` positions past the runner.
    fn claim(&self, seed: u64) -> Option<(Slot, Box<dyn Provider>)> {
        let mut state = self.locked();
        loop {
            if state.closed {
                return None;
            }
            if let Some((number, choices)) = state.replays.pop_front() {
                return Some((Slot::Replay(number), Box::new(Replaying::new(choices))));
            }
            if state.next < state.taken + state.workers {
                let position = state.next;
                state.next += 1;
                let provider = match random_at(position) {
                    Some(index) => generating(seed, index),
                    None => edge(position / 2),
                };
                return Some((Slot::Position(position), provider));
            }
            state = self.wait(state);
        }
    }

    /// Returns the case at `position` once it has ended.
    fn take(&self, position: usize) -> Result<Execution, Fault> {
        let mut state = self.locked();
        state.taken = state.taken.max(position + 1);
        self.changed.notify_all();
        self.ended(state, Slot::Position(position))
    }

    /// Runs the case that replays `choices` on the next free worker.
    fn replay(&self, choices: Vec<Choice>) -> Result<Execution, Fault> {
        let mut state = self.locked();
        let number = state.replayed;
        state.replayed += 1;
        state.replays.push_back((number, choices));
        self.changed.notify_all();
        self.ended(state, Slot::Replay(number))
    }

    /// Returns the case of `slot` once it has ended.
    fn ended(&self, mut state: MutexGuard<'_, State>, slot: Slot) -> Result<Execution, Fault> {
        loop {
            if let Some(result) = state.done.remove(&slot) {
                return result;
            }
            state = self.wait(state);
        }
    }

    /// Ends the run: the workers finish their cases and stop.
    fn close(&self) {
        self.locked().closed = true;
        self.changed.notify_all();
    }
}
