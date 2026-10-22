// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The task scheduler: which ready task runs next, decided by the choices of
//! a case.

use super::strategy::Strategy;
use crate::prop::Case;
use crate::prop::engine::case::{Request, Stop};
use crate::prop::engine::choice::{Bounds, IntegerBounds, UINT64_MAX};
use crate::prop::engine::draw::Draw;
use std::collections::VecDeque;
use std::fmt;
use std::future::Future;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::task::{Context, Poll, Wake, Waker};

/// A task that a subject spawns, which makes its future from the scheduler
/// that releases it.
type Spawned<'c> = Box<dyn for<'s> FnOnce(&'s Scheduler<'c>) -> Local<'s> + Send + 'c>;

/// A task of a run, which stays on the thread of the run.
pub(crate) type Local<'t> = Pin<Box<dyn Future<Output = ()> + 't>>;

/// The flag that a task's waker sets.
struct Woken(AtomicBool);

impl Wake for Woken {
    fn wake(self: Arc<Self>) {
        self.0.store(true, Ordering::SeqCst);
    }
}

/// A task that a run holds: its number in the order of spawning, its future,
/// its priority under PCT, its level, and the flag that its waker sets.
struct Held<'t> {
    number: usize,
    future: Local<'t>,
    priority: i128,
    level: i64,
    woken: Arc<Woken>,
}

/// How one turn of a task ended.
enum Turn {
    /// The task awaited a yield of the scheduler.
    Yielded,
    /// The task returned.
    Returned,
    /// The task awaits a future of something other than the scheduler.
    Waiting,
}

/// The state of a scheduler.
#[derive(Default)]
struct State<'c> {
    /// The tasks spawned since the last turn, with their numbers and their
    /// priorities.
    spawned: Vec<(usize, Spawned<'c>, i128)>,
    /// The tasks spawned so far.
    count: usize,
    /// Whether a turn of a task runs.
    turn: bool,
    /// Whether the task of the running turn has yielded.
    yielded: bool,
    /// Whether a run releases tasks.
    running: bool,
    /// The level of the task that the last change point lowered.
    lowest: i64,
}

/// Releases the tasks of one case, one at a time, in an order that the
/// case's choices decide, so a schedule replays and shrinks as the case's
/// values do.
///
/// A task is a future that [`Scheduler::spawn`] makes ready, and that runs
/// only while the scheduler releases it: until it awaits
/// [`Scheduler::yield_now`], or until it returns. [`Scheduler::run`] makes
/// the choice of each release, and polls each released task on the thread
/// that calls it. A subject that runs as tasks spawns them through the
/// scheduler, and awaits `yield_now` at each point where another task may
/// run:
///
/// ```
/// use dokimi_assert::stateful::{self, Scheduler};
/// use dokimi_assert::{Collector, check, prop};
/// use std::sync::Mutex;
///
/// let t = Collector::new();
/// prop::for_all(&t, "two tasks interleave", prop::cases(20), |c| {
///     let steps = Mutex::new(Vec::new());
///     let scheduler = Scheduler::new(c, stateful::uniform());
///     for name in ["a", "b"] {
///         let steps = &steps;
///         scheduler.spawn(async move |s| {
///             steps.lock().unwrap().push(name);
///             s.yield_now().await;
///             steps.lock().unwrap().push(name);
///         });
///     }
///     scheduler.run();
///     check::length(c, &*steps.lock().unwrap(), 4, "each task takes two steps");
/// });
/// ```
///
/// A task that awaits a future of something other than the scheduler waits
/// until another task wakes it. When no task is ready and a task waits, the
/// run ends the case with a fault that lists the waiting tasks.
pub struct Scheduler<'c> {
    case: &'c Case<'c>,
    strategy: Strategy,
    state: Mutex<State<'c>>,
}

impl fmt::Debug for Scheduler<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Scheduler")
            .field("strategy", &self.strategy)
            .finish_non_exhaustive()
    }
}

/// The future of [`Scheduler::yield_now`].
#[derive(Debug)]
#[must_use = "a yield ends the task's turn only when the task awaits it"]
pub struct YieldNow<'a, 'c> {
    scheduler: &'a Scheduler<'c>,
    yielded: bool,
}

impl Future for YieldNow<'_, '_> {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<()> {
        if self.yielded {
            return Poll::Ready(());
        }
        let mut state = self.scheduler.locked();
        if !state.turn {
            return Poll::Ready(());
        }
        state.yielded = true;
        drop(state);
        self.yielded = true;
        Poll::Pending
    }
}

impl<'c> Scheduler<'c> {
    /// Returns a scheduler that releases the tasks of `case` by `strategy`.
    pub fn new(case: &'c Case<'_>, strategy: Strategy) -> Self {
        Self {
            case,
            strategy,
            state: Mutex::default(),
        }
    }

    fn locked(&self) -> MutexGuard<'_, State<'c>> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Returns an integer choice of the case, and ends the case when it
    /// cannot make one.
    fn integer(&self, bounds: IntegerBounds, edge: Option<i128>) -> i128 {
        match self.case.integer(bounds, edge, false) {
            Ok(value) => value,
            Err(stop) => self.case.end(stop),
        }
    }

    /// Returns the priority of a task under PCT, a choice of the case, and 0
    /// under the uniform strategy.
    fn priority(&self) -> i128 {
        if self.strategy.depth == 0 {
            return 0;
        }
        self.integer(IntegerBounds::of(0, UINT64_MAX), None)
    }

    /// Makes `task` ready after every ready task. Under PCT, the task first
    /// receives its priority, a choice of the case. The task does not run
    /// until a release starts it. `task` is an async closure that receives
    /// the scheduler, so the task awaits [`Scheduler::yield_now`] of the
    /// scheduler that releases it.
    pub fn spawn(&self, task: impl AsyncFnOnce(&Scheduler<'c>) + Send + 'c) {
        let priority = self.priority();
        let made: Spawned<'c> = Box::new(move |scheduler| Box::pin(task(scheduler)));
        let mut state = self.locked();
        let number = state.count;
        state.count += 1;
        state.spawned.push((number, made, priority));
    }

    /// Returns a future that ends the turn of the running task, which becomes
    /// ready again after every ready task and continues at its next release.
    /// Outside a task's turn, such as on client 0 of a machine's steps, the
    /// future is ready at once.
    pub fn yield_now(&self) -> YieldNow<'_, 'c> {
        YieldNow {
            scheduler: self,
            yielded: false,
        }
    }

    /// Makes the change points of PCT, and then releases one ready task at a
    /// time until none is ready. A task may spawn others while it runs. A task
    /// that panics ends the run: the run drops the other tasks and raises the
    /// panic again on the calling thread.
    ///
    /// # Panics
    ///
    /// Panics for a call by a task of the scheduler while a run releases it,
    /// and raises the panic of a task.
    pub fn run(&self) {
        self.execute(Vec::new());
    }

    /// Returns the counts of the change points of a run: up to `depth − 1`
    /// under PCT, and none under the uniform strategy.
    fn change_points(&self) -> Vec<i128> {
        let mut points = Vec::new();
        for _ in 1..self.strategy.depth {
            if self.integer(IntegerBounds::of(0, 1), Some(1)) == 1 {
                points.push(self.integer(IntegerBounds::of(0, UINT64_MAX), None));
            }
        }
        points
    }

    /// Removes the task to release from the ready tasks and returns it: under
    /// PCT the task of the highest level and then the highest priority, the
    /// earliest ready among equals, and otherwise the case's choice among the
    /// ready tasks.
    fn next<'m>(&self, ready: &mut VecDeque<Held<'m>>) -> Option<Held<'m>> {
        if ready.is_empty() {
            return None;
        }
        let index = if self.strategy.depth > 0 {
            let mut best = 0;
            for (at, task) in ready.iter().enumerate() {
                let top = &ready[best];
                if task.level > top.level
                    || (task.level == top.level && task.priority > top.priority)
                {
                    best = at;
                }
            }
            best
        } else {
            let count = ready.len();
            let request = Request {
                bounds: Bounds::Integer(IntegerBounds::of(
                    0,
                    i128::try_from(count).unwrap_or(i128::MAX) - 1,
                )),
                draw: Draw::Below(count),
                edge: Some(0),
                reuse: false,
            };
            match self.case.choose(request) {
                Ok(choice) => {
                    usize::try_from(choice.as_integer().unwrap_or_default()).unwrap_or_default()
                }
                Err(stop) => self.case.end(stop),
            }
        };
        ready.remove(index)
    }

    /// Runs one turn of `task`: polls it once, and returns how the turn
    /// ended, or the payload of its panic.
    fn turn(&self, task: &mut Held<'_>) -> std::thread::Result<Turn> {
        {
            let mut state = self.locked();
            state.turn = true;
            state.yielded = false;
        }
        task.woken.0.store(false, Ordering::SeqCst);
        let waker = Waker::from(Arc::clone(&task.woken));
        let mut context = Context::from_waker(&waker);
        let polled = catch_unwind(AssertUnwindSafe(|| task.future.as_mut().poll(&mut context)));
        let mut state = self.locked();
        state.turn = false;
        polled.map(|poll| match poll {
            Poll::Ready(()) => Turn::Returned,
            Poll::Pending if state.yielded => Turn::Yielded,
            Poll::Pending => Turn::Waiting,
        })
    }

    /// Releases `first`, after the tasks spawned before the run, and every
    /// task that they spawn, until none is ready.
    pub(crate) fn execute<'m>(&'m self, first: Vec<Local<'m>>) {
        let mut ready: VecDeque<Held<'m>> = VecDeque::new();
        {
            let mut state = self.locked();
            assert!(
                !state.running,
                "stateful: a task called run of its own scheduler"
            );
            state.running = true;
        }
        self.admit(&mut ready);
        for future in first {
            let priority = self.priority();
            let mut state = self.locked();
            let number = state.count;
            state.count += 1;
            drop(state);
            ready.push_back(held(number, future, priority));
        }
        let points = self.change_points();
        let mut waiting: Vec<Held<'m>> = Vec::new();
        let mut release: i128 = 0;
        while let Some(mut task) = self.next(&mut ready) {
            for point in &points {
                if *point == release {
                    let mut state = self.locked();
                    state.lowest -= 1;
                    task.level = state.lowest;
                }
            }
            let turn = self.turn(&mut task);
            self.admit(&mut ready);
            match turn {
                Ok(Turn::Yielded) => ready.push_back(task),
                Ok(Turn::Returned) => {}
                Ok(Turn::Waiting) => waiting.push(task),
                Err(payload) => {
                    drop((ready, waiting));
                    self.stop();
                    resume_unwind(payload);
                }
            }
            let (woken, still): (Vec<Held<'m>>, Vec<Held<'m>>) = waiting
                .into_iter()
                .partition(|task| task.woken.0.load(Ordering::SeqCst));
            ready.extend(woken);
            waiting = still;
            release += 1;
        }
        self.stop();
        if !waiting.is_empty() {
            let numbers: Vec<String> = waiting.iter().map(|task| task.number.to_string()).collect();
            self.case.end(Stop::Fault(format!(
                "stateful: the tasks {} wait on a future that no task of the scheduler wakes",
                numbers.join(", ")
            )));
        }
    }

    /// Moves the tasks spawned since the last turn into the ready tasks, in
    /// the order of spawning.
    fn admit<'m>(&'m self, ready: &mut VecDeque<Held<'m>>) {
        let spawned = std::mem::take(&mut self.locked().spawned);
        ready.extend(
            spawned
                .into_iter()
                .map(|(number, make, priority)| held(number, make(self), priority)),
        );
    }

    /// Ends a run.
    fn stop(&self) {
        let mut state = self.locked();
        state.running = false;
        state.spawned.clear();
    }
}

/// Returns a task that a run holds.
fn held(number: usize, future: Local<'_>, priority: i128) -> Held<'_> {
    Held {
        number,
        future,
        priority,
        level: 0,
        woken: Arc::new(Woken(AtomicBool::new(false))),
    }
}

/// A scheduler that runs the clients of a concurrent section.
pub(crate) trait Section {
    /// Runs `tasks`, one per client, as tasks of the scheduler.
    fn run_section<'m>(&'m self, tasks: Vec<Local<'m>>);
}

impl Section for Scheduler<'_> {
    fn run_section<'m>(&'m self, tasks: Vec<Local<'m>>) {
        self.execute(tasks);
    }
}
