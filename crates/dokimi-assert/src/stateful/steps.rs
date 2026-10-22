// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The steps of a machine in a case.

use super::machine::{Action, Machine, Planned};
use super::options::Options;
use super::scheduler::Local;
use crate::failure::Where;
use crate::history::linearizable::{self, whole};
use crate::matcher::panics::intern;
use crate::prop::Case;
use crate::prop::engine::case::{Place, Request, Step, Stop};
use crate::prop::engine::choice::{Bounds, IntegerBounds};
use crate::prop::engine::collection::{Sizes, more};
use crate::prop::engine::draw::{Draw, HALF};
use crate::prop::engine::trace::TraceError;
use crate::prop::scope::{current, enter};
use crate::seat::Seat;
use std::fmt::Debug;
use std::future::Future;
use std::panic::{Location, resume_unwind};
use std::pin::pin;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};
use std::thread::{self, Thread};

/// The contract of the check after a step.
const CONTRACT: &str = "the history of the machine's steps is linearizable";

/// Runs `future` to its end on the calling thread, which parks while the
/// future waits.
fn block_on<F: Future>(future: F) -> F::Output {
    /// The waker that unparks the thread of a `block_on`.
    struct Unpark(Thread);

    impl Wake for Unpark {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }

    let mut future = pin!(future);
    let waker = Waker::from(Arc::new(Unpark(thread::current())));
    let mut context = Context::from_waker(&waker);
    loop {
        if let Poll::Ready(output) = future.as_mut().poll(&mut context) {
            return output;
        }
        thread::park();
    }
}

/// Restores the place that a case had before a machine's steps, however the
/// steps end.
struct Restore<'c> {
    case: &'c Case<'c>,
    before: Option<Place>,
}

impl Drop for Restore<'_> {
    fn drop(&mut self) {
        let before = self.before.take();
        self.case.with_record(|record| record.place = before);
    }
}

/// One run of a machine's steps in one case.
struct Run<'r, 'a, S> {
    case: &'r Case<'r>,
    machine: &'r Machine<'a, S>,
    options: Options<'r>,
    /// Whether the case follows a trace.
    traced: bool,
    /// The states that the last check left, and one default state before a
    /// check and for a machine without a spec.
    states: Vec<S>,
    /// The call site of the steps, at which the record of a check states the
    /// failure.
    site: Where,
}

/// Runs the steps of `machine` in the case `case`, in six parts, and records
/// every decision as a choice of the case:
///
/// 1. Swarm: one choice per action, in order, whether the case keeps it,
///    with odds of 1 in 2 conditioned on keeping one. A case keeps every
///    action under `swarm(false)`.
/// 2. Setup: the check of the history, and the invariant.
/// 3. Sequential steps on client 0. Before each step, the kept actions that
///    are enabled in every state that the last check left. With none, the
///    steps end. Otherwise a continue flag, with the mean of `mean` and the
///    most steps of `max`, and an index into the list by weight. The step is
///    a span labelled with the action's name, from its flag to the end of its
///    run, which requests the step's input and runs the action. The check and
///    the invariant follow each step.
/// 4. A concurrent section, for `clients` of 2 or more. The section lists
///    its steps before any runs: each a flag with the most steps of
///    `concurrent`, a client from 0 to the clients, an index into the kept
///    actions without `enabled`, and the step's input. Client 0 runs its
///    steps, and then the other clients run theirs: as tasks of the
///    scheduler of `tasks`, one task per client that has a step, spawned in
///    client order, or else on threads, and then the case runs up to
///    `repeat` times, because threads do not replay. The check follows.
/// 5. The drain: steps of the actions with `drain`, without a flag, until
///    none is enabled or the most steps ran, each followed by the check and
///    the invariant.
/// 6. Settle, and the invariant.
///
/// The check searches the case's history with the machine's spec, with
/// every call in one partition, and the next state is the first of the
/// states that the order it found leaves. A check that fails, violated or
/// undecided, ends the case with the record of `linearizable`, whose
/// contract states that the history of the machine's steps is linearizable.
///
/// The case of `prop::draws` follows its trace: the swarm keeps the actions
/// that its step entries name, and each step entry becomes the choices of
/// one step. `steps` refuses a step entry that it cannot take at its
/// position, which ends the run with a fault at the entry: an action that
/// the step's list lacks, a step past its part's most steps, a client outside
/// the clients, a concurrent step of a machine with one client, and a drain
/// step after the drain.
///
/// ```
/// use dokimi_assert::history::{Operation, Spec};
/// use dokimi_assert::stateful::{self, Action, Machine};
/// use dokimi_assert::{Collector, Value, prop};
/// use std::sync::atomic::{AtomicI64, Ordering};
///
/// /// A counter: increment returns the new count.
/// struct Counter;
///
/// impl Spec for Counter {
///     type State = i64;
///
///     fn initial(&self) -> i64 {
///         0
///     }
///
///     fn next(&self, state: &i64, op: &Operation) -> Vec<i64> {
///         if op.returned(*state + 1) { vec![*state + 1] } else { Vec::new() }
///     }
/// }
///
/// let t = Collector::new();
/// prop::for_all(&t, "the counter counts", prop::cases(20), |c| {
///     let count = AtomicI64::new(0);
///     let machine = Machine::new([Action::new("increment", async |c, client| {
///         let call = c.history().invoke(client, "increment", [], []);
///         call.ok(count.fetch_add(1, Ordering::SeqCst) + 1);
///     })])
///     .spec(Counter);
///     stateful::steps(c, &machine, stateful::max(10));
/// });
/// ```
#[track_caller]
pub fn steps<S>(case: &Case<'_>, machine: &Machine<'_, S>, options: Options<'_>)
where
    S: Clone + PartialEq + Debug + Default + Send + Sync + 'static,
{
    let site = Where::from(Location::caller());
    let traced = case.with_record(|record| record.provider().tracing().is_some());
    let _restore = Restore {
        case,
        before: case.with_record(|record| record.place.clone()),
    };
    let mut run = Run {
        case,
        machine,
        options,
        traced,
        states: vec![S::default()],
        site,
    };
    run.steps();
}

impl<'r, 'a, S> Run<'r, 'a, S>
where
    S: Clone + PartialEq + Debug + Default + Send + Sync + 'static,
{
    /// Runs the six parts in order.
    fn steps(&mut self) {
        let kept = self.swarm();
        self.at("setup", None, None);
        self.check();
        self.invariant();
        self.sequential(&kept);
        self.concurrent(&kept);
        self.drain();
        self.at("settle", None, None);
        if let Some(settle) = &self.machine.settle {
            settle(self.case, &self.states[0]);
        }
        self.invariant();
    }

    /// Makes the case's place the step at `position` of `part`, with its
    /// action once the step has chosen it.
    fn at(&self, part: &'static str, position: Option<usize>, action: Option<&str>) {
        let place = Place {
            part,
            position,
            action: action.map(str::to_owned),
        };
        self.case.with_record(|record| record.place = Some(place));
    }

    /// Returns the value of `request`, and ends the case when it cannot make
    /// one.
    fn choose(&self, request: Request) -> i128 {
        match self.case.choose(request) {
            Ok(choice) => choice.as_integer().unwrap_or_default(),
            Err(stop) => self.case.end(stop),
        }
    }

    /// Returns the case's decision whether a part takes another step.
    fn more(&self, count: usize, sizes: Sizes, mean: Option<usize>) -> bool {
        match more(self.case, count, sizes, false, mean) {
            Ok(more) => more,
            Err(stop) => self.case.end(stop),
        }
    }

    /// Runs `follow` on the trace that the case follows, and ends the case at
    /// the entry that `follow` refuses.
    fn follow(
        &self,
        follow: impl FnOnce(&mut crate::prop::engine::trace::Tracing) -> Result<(), TraceError>,
    ) {
        let refused = self
            .case
            .with_record(|record| record.provider().tracing().map(follow));
        if let Some(Err(error)) = refused {
            self.case.end(Stop::Trace(error));
        }
    }

    /// Returns the actions that the case keeps, in order. A case that follows
    /// a trace keeps the actions that its step entries name.
    fn swarm(&self) -> Vec<&'r Action<'a, S>> {
        let actions = &self.machine.actions;
        if !self.options.swarm {
            return actions.iter().collect();
        }
        let named = self.case.with_record(|record| {
            record
                .provider()
                .tracing()
                .map(|tracing| tracing.actions())
                .unwrap_or_default()
        });
        let mut kept = Vec::new();
        for (position, action) in actions.iter().enumerate() {
            self.at("swarm", Some(position), Some(&action.name));
            if self.traced {
                let prepared = i128::from(named.contains(&action.name));
                self.follow(|tracing| {
                    tracing.prepare(&[prepared]);
                    Ok(())
                });
            }
            let earlier = !kept.is_empty();
            let remaining = actions.len() - position;
            let lo = i128::from(!earlier && remaining == 1);
            let request = Request {
                bounds: Bounds::Integer(IntegerBounds::of(lo, 1)),
                draw: Draw::Keep {
                    probability: HALF,
                    remaining,
                    kept: earlier,
                },
                edge: Some(1),
                reuse: false,
            };
            if self.choose(request) == 1 {
                kept.push(action);
            }
        }
        kept
    }

    /// Returns the actions of `listed` that are enabled in every state that
    /// the last check left.
    fn available<'l>(&self, listed: &[&'l Action<'a, S>]) -> Vec<&'l Action<'a, S>> {
        listed
            .iter()
            .filter(|action| {
                action
                    .enabled
                    .as_ref()
                    .is_none_or(|enabled| self.states.iter().all(enabled))
            })
            .copied()
            .collect()
    }

    /// Returns the case's choice of an action of `listed`, by weight.
    fn index(&self, listed: &[&Action<'a, S>]) -> usize {
        let weights: Arc<[u64]> = listed.iter().map(|action| action.weight).collect();
        let request = Request {
            bounds: Bounds::Integer(IntegerBounds::of(
                0,
                i128::try_from(listed.len()).unwrap_or(i128::MAX) - 1,
            )),
            draw: Draw::Weighted(weights),
            edge: Some(0),
            reuse: false,
        };
        usize::try_from(self.choose(request)).unwrap_or_default()
    }

    /// Takes sequential steps until no kept action is enabled or a flag stops
    /// them.
    fn sequential(&mut self, kept: &[&'r Action<'a, S>]) {
        let (max, mean) = (self.options.max, self.options.mean);
        for count in 0.. {
            self.at("sequential", Some(count), None);
            let available = self.available(kept);
            let start = self.case.len();
            if self.traced {
                self.follow(|tracing| {
                    let Some(step) = tracing
                        .next_step()
                        .filter(|step| step.client.is_none() && !step.drain)
                    else {
                        return Ok(());
                    };
                    let index = available
                        .iter()
                        .position(|action| action.name == step.action);
                    match index.filter(|_| count < max) {
                        Some(index) => {
                            tracing.take(&[1, i128::try_from(index).unwrap_or_default()]);
                            Ok(())
                        }
                        None => Err(tracing.refuse()),
                    }
                });
            }
            if available.is_empty() || !self.more(count, Sizes::up_to(max), Some(mean)) {
                return;
            }
            self.take(start, &available, false, count);
        }
    }

    /// Lists the steps of the concurrent section, runs them, and checks the
    /// history.
    fn concurrent(&mut self, kept: &[&'r Action<'a, S>]) {
        let (clients, most) = (self.options.clients, self.options.concurrent);
        if clients == 1 {
            if self.traced {
                self.follow(|tracing| match tracing.next_step() {
                    Some(step) if step.client.is_some() && !step.drain => Err(tracing.refuse()),
                    _ => Ok(()),
                });
            }
            return;
        }
        let eligible: Vec<&'r Action<'a, S>> = kept
            .iter()
            .filter(|action| action.enabled.is_none())
            .copied()
            .collect();
        let mut planned: Vec<Vec<Planned<'_>>> = (0..=clients).map(|_| Vec::new()).collect();
        for count in 0.. {
            self.at("concurrent", Some(count), None);
            let start = self.case.len();
            if self.traced {
                self.follow(|tracing| {
                    let Some(step) = tracing.next_step().filter(|step| !step.drain) else {
                        return Ok(());
                    };
                    let Some(client) = step.client else {
                        return Ok(());
                    };
                    let index = eligible
                        .iter()
                        .position(|action| action.name == step.action);
                    match index.filter(|_| count < most && client <= clients) {
                        Some(index) => {
                            let values = [
                                1,
                                i128::try_from(client).unwrap_or_default(),
                                i128::try_from(index).unwrap_or_default(),
                            ];
                            tracing.take(&values);
                            Ok(())
                        }
                        None => Err(tracing.refuse()),
                    }
                });
            }
            if eligible.is_empty() || !self.more(count, Sizes::up_to(most), None) {
                break;
            }
            let client = self.client();
            let action = eligible[self.index(&eligible)];
            self.at("concurrent", Some(count), Some(&action.name));
            let _span = self.case.span(intern(&action.name), Some(start));
            self.case.with_record(|record| {
                record.step(Step {
                    action: action.name.clone(),
                    client: Some(client),
                    drain: false,
                });
            });
            planned[client].push(action.plan(self.case, &self.states[0]));
        }
        self.at("concurrent", None, None);
        self.section(planned);
        self.check();
    }

    /// Returns the case's choice of a concurrent step's client, uniform over
    /// the clients with target 0.
    fn client(&self) -> usize {
        let clients = self.options.clients;
        let request = Request {
            bounds: Bounds::Integer(IntegerBounds::of(
                0,
                i128::try_from(clients).unwrap_or(i128::MAX),
            )),
            draw: Draw::Below(clients + 1),
            edge: Some(0),
            reuse: false,
        };
        usize::try_from(self.choose(request)).unwrap_or_default()
    }

    /// Runs the planned steps of client 0, and then those of the other
    /// clients: as tasks of the scheduler, or on threads.
    fn section(&self, mut planned: Vec<Vec<Planned<'r>>>) {
        let case = self.case;
        for step in planned.remove(0) {
            block_on(step(case, 0));
        }
        let active: Vec<(usize, Vec<Planned<'r>>)> = planned
            .into_iter()
            .enumerate()
            .map(|(at, steps)| (at + 1, steps))
            .filter(|(_, steps)| !steps.is_empty())
            .collect();
        if let Some(scheduler) = self.options.scheduler {
            let tasks: Vec<Local<'r>> = active
                .into_iter()
                .map(|(client, steps)| -> Local<'r> {
                    Box::pin(async move {
                        for step in steps {
                            step(case, client).await;
                        }
                    })
                })
                .collect();
            scheduler.run_section(tasks);
            return;
        }
        if active.is_empty() {
            return;
        }
        let repeat = self.options.repeat;
        case.with_record(|record| record.repeats = repeat);
        // The draws of a client pick the generators of the run.
        let using = current();
        let ended: Vec<thread::Result<()>> = thread::scope(|scope| {
            let handles: Vec<_> = active
                .into_iter()
                .map(|(client, steps)| {
                    let using = using.clone();
                    scope.spawn(move || {
                        let _run = enter(using);
                        for step in steps {
                            block_on(step(case, client));
                        }
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(thread::ScopedJoinHandle::join)
                .collect()
        });
        if let Some(Err(payload)) = ended.into_iter().find(Result::is_err) {
            resume_unwind(payload);
        }
    }

    /// Takes drain steps until no drain action is enabled or the most steps
    /// ran.
    fn drain(&mut self) {
        let drains: Vec<&'r Action<'a, S>> = self
            .machine
            .actions
            .iter()
            .filter(|action| action.drain)
            .collect();
        for count in 0.. {
            self.at("drain", Some(count), None);
            let available = if count < self.options.max {
                self.available(&drains)
            } else {
                Vec::new()
            };
            if self.traced {
                self.follow(|tracing| {
                    let Some(step) = tracing.next_step().filter(|step| step.drain) else {
                        return Ok(());
                    };
                    match available
                        .iter()
                        .position(|action| action.name == step.action)
                    {
                        Some(index) => {
                            tracing.take(&[i128::try_from(index).unwrap_or_default()]);
                            Ok(())
                        }
                        None => Err(tracing.refuse()),
                    }
                });
            }
            if available.is_empty() {
                return;
            }
            let start = self.case.len();
            self.take(start, &available, true, count);
        }
    }

    /// Takes the step at `position` of the sequential part or the drain, on
    /// client 0, of the action of `listed` that the case's index chooses: a
    /// span labelled with the action's name from `start`, which records the
    /// step, requests its input and runs it, and then the check and the
    /// invariant.
    fn take(&mut self, start: usize, listed: &[&'r Action<'a, S>], drain: bool, position: usize) {
        let action = listed[self.index(listed)];
        let part = if drain { "drain" } else { "sequential" };
        self.at(part, Some(position), Some(&action.name));
        {
            let _span = self.case.span(intern(&action.name), Some(start));
            self.case.with_record(|record| {
                record.step(Step {
                    action: action.name.clone(),
                    client: None,
                    drain,
                });
            });
            let step = action.plan(self.case, &self.states[0]);
            block_on(step(self.case, 0));
        }
        self.check();
        self.invariant();
    }

    /// Runs the machine's invariant, when it has one.
    fn invariant(&self) {
        if let Some(invariant) = &self.machine.invariant {
            invariant(self.case, &self.states[0]);
        }
    }

    /// Checks the case's history with the machine's spec, with every call in
    /// one partition, and keeps the states that the order it found leaves. A
    /// check that fails ends the case with its record. A machine without a
    /// spec checks nothing.
    fn check(&mut self) {
        let Some(spec) = &self.machine.spec else {
            return;
        };
        match whole(self.case.history(), spec.as_ref()) {
            Err(panic) => {
                let (at, reason) = panic.place();
                let place = if at.is_empty() {
                    String::new()
                } else {
                    format!("{at}: ")
                };
                self.case
                    .end(Stop::Fault(format!("stateful::steps: {place}{reason}")));
            }
            Ok((detail, states)) => {
                if detail.outcome != linearizable::Verdict::Passed {
                    let failure = linearizable::record(&detail, CONTRACT, Some(self.site));
                    self.case.report(&failure, true);
                }
                self.states = states;
            }
        }
    }
}
