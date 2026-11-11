// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The task scheduler: which ready task runs next, decided by the choices of
//! a case, and the clock on which its tasks sleep.

use super::strategy::Strategy;
use crate::clock::Clock;
use crate::prop::Case;
use crate::prop::engine::case::{Request, Stop};
use crate::prop::engine::choice::{Bounds, IntegerBounds, UINT64_MAX};
use crate::prop::engine::draw::Draw;
use crate::seat::Seat;
use std::any::Any;
use std::collections::VecDeque;
use std::fmt;
use std::future::Future;
use std::mem::take;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::task::{Context, Poll, Wake, Waker};
use std::time::Duration;

/// The time at which the clock of every scheduler starts: midnight UTC
/// 2000-01-01, as the time since the Unix epoch.
const EPOCH: Duration = Duration::from_secs(946_684_800);

/// The panic message of a wait outside a task for a condition that is false.
const WAIT_OUTSIDE: &str = "stateful: a wait outside a task waits for a condition that is false";

/// The panic message of a sleep of a positive duration outside a task, and
/// of a blocking sleep of a positive duration.
const SLEEP_OUTSIDE: &str =
    "stateful: a sleep outside a task waits for a clock that only the body moves";

/// The future of a task that a subject spawns. The scheduler keeps it from
/// one call to the next.
type Task<'c> = Pin<Box<dyn Future<Output = ()> + Send + 'c>>;

/// The future of a client of a concurrent section, which runs on the thread
/// of the section.
pub(crate) type Local<'t> = Pin<Box<dyn Future<Output = ()> + 't>>;

/// The condition of a wait.
type Condition<'c> = Arc<dyn Fn() -> bool + Send + Sync + 'c>;

/// The flag that a task's waker sets.
struct Woken(AtomicBool);

impl Wake for Woken {
    fn wake(self: Arc<Self>) {
        self.0.store(true, Ordering::SeqCst);
    }
}

/// What a parked task waits for.
enum Park<'c> {
    /// The deadline of a sleep, as the time since the Unix epoch.
    Sleep(Duration),
    /// The condition of a wait.
    Wait(Condition<'c>),
}

/// A task of the scheduler. It has a number in the order of spawning, a
/// future `W`, a priority under PCT, a level and a flag that its waker sets.
struct Held<W> {
    number: usize,
    work: W,
    priority: i128,
    level: i64,
    woken: Arc<Woken>,
}

impl<'c> Held<Task<'c>> {
    /// Returns the task as the work of a call.
    fn into_work<'m>(self) -> Held<Work<'c, 'm>> {
        Held {
            number: self.number,
            work: Work::Task(self.work),
            priority: self.priority,
            level: self.level,
            woken: self.woken,
        }
    }
}

impl<'c> Held<Work<'c, '_>> {
    /// Returns the task of the scheduler, or `None` for a client of a
    /// section.
    fn into_task(self) -> Option<Held<Task<'c>>> {
        match self.work {
            Work::Task(future) => Some(Held {
                number: self.number,
                work: future,
                priority: self.priority,
                level: self.level,
                woken: self.woken,
            }),
            Work::Client(_) => None,
        }
    }
}

/// The future that one call releases: a task of the scheduler, or a client
/// of a concurrent section.
enum Work<'c, 'm> {
    Task(Task<'c>),
    Client(Local<'m>),
}

impl Work<'_, '_> {
    fn poll(&mut self, context: &mut Context<'_>) -> Poll<()> {
        match self {
            Work::Task(future) => future.as_mut().poll(context),
            Work::Client(future) => future.as_mut().poll(context),
        }
    }
}

/// How one turn of a task ended.
enum Turn<'c> {
    /// The task awaited a yield of the scheduler.
    Yielded,
    /// The task returned.
    Returned,
    /// The task awaited a sleep or a wait of the scheduler.
    Parked(Park<'c>),
    /// The task awaits a future of something other than the scheduler.
    Waiting,
}

/// Why a call ended every task while a task was still ready.
enum Halt {
    /// A task or a condition panicked with this payload.
    Panicked(Box<dyn Any + Send>),
    /// The call released the limit of its strategy while a task was ready.
    Limit,
}

/// The tasks of one call of the scheduler.
struct Queue<'c, 'm> {
    /// The ready tasks, in the order that they became ready.
    ready: VecDeque<Held<Work<'c, 'm>>>,
    /// The tasks that wait or sleep, in the order that they parked.
    parked: Vec<(Held<Work<'c, 'm>>, Park<'c>)>,
    /// The tasks that await a future of something other than the scheduler.
    waiting: Vec<Held<Work<'c, 'm>>>,
}

impl Queue<'_, '_> {
    /// Returns the earliest deadline of a sleeping task, or `end` when no
    /// deadline is earlier.
    fn earliest(&self, end: Duration) -> Duration {
        self.parked
            .iter()
            .filter_map(|(_, park)| match park {
                Park::Sleep(deadline) => Some(*deadline),
                Park::Wait(_) => None,
            })
            .fold(end, Duration::min)
    }
}

/// The state of a scheduler.
#[derive(Default)]
struct State<'c> {
    /// The tasks spawned since the last turn, in the order of spawning.
    spawned: Vec<Held<Task<'c>>>,
    /// The tasks that wait or sleep between two calls, in the order that
    /// they parked.
    parked: Vec<(Held<Task<'c>>, Park<'c>)>,
    /// The time of the clock since the Unix epoch.
    now: Duration,
    /// The tasks spawned so far.
    count: usize,
    /// Whether a turn of a task runs.
    turn: bool,
    /// Whether the task of the running turn has yielded.
    yielded: bool,
    /// The sleep or the wait of the task of the running turn.
    parking: Option<Park<'c>>,
    /// Whether a call releases tasks.
    running: bool,
    /// The level of the task that the last change point lowered.
    lowest: i64,
}

/// The state that a scheduler shares with the handles of its tasks and with
/// its clock.
struct Shared<'c> {
    case: &'c Case<'c>,
    strategy: Strategy,
    state: Mutex<State<'c>>,
}

impl<'c> Shared<'c> {
    fn locked(&self) -> MutexGuard<'_, State<'c>> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Releases the tasks of one case, one at a time, in an order that the
/// case's choices decide, so a schedule replays and shrinks as the case's
/// values do.
///
/// A task is a future that [`Scheduler::spawn`] makes ready, and that runs
/// only while the scheduler releases it: until it awaits
/// [`Scheduler::yield_now`], [`Scheduler::wait`] or the sleep of
/// [`Scheduler::clock`], or until it returns. [`Scheduler::run`] makes the
/// choice of each release, and polls each released task on the thread that
/// calls it. A task that awaits a wait or a sleep is parked and not ready. It
/// remains parked after the call returns, until a later run or
/// [`Scheduler::advance`] makes it ready. A subject that runs as tasks spawns
/// them through the scheduler, and awaits `yield_now` at each point where
/// another task may run:
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
///         scheduler.spawn(move |s| async move {
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
/// The scheduler checks its parked tasks at the start of each run and each
/// advance, after each turn, and after each move of its clock. The check
/// makes ready, after every ready task, each sleeping task whose deadline is
/// the time of the clock or earlier, by deadline and then in the order of the
/// sleeps. Then it makes ready each waiting task whose condition returns
/// true, in the order of the waits. A task that yields joins the ready tasks
/// before the tasks that the check after its turn makes ready.
///
/// A task that awaits a future of something other than the scheduler waits
/// until another task wakes it. When no task is ready and a task waits, the
/// call ends the case with a fault that lists the waiting tasks.
///
/// The scheduler that [`Scheduler::new`] returns ends every task that has not
/// returned when it drops, so a parked task does not return from its wait or
/// its sleep. Declare the state that a task borrows before the scheduler, so
/// that the scheduler drops first.
pub struct Scheduler<'c> {
    shared: Arc<Shared<'c>>,
    /// Whether `new` returned the scheduler. A handle of a task is not the
    /// owner.
    owner: bool,
}

impl fmt::Debug for Scheduler<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Scheduler")
            .field("strategy", &self.shared.strategy)
            .finish_non_exhaustive()
    }
}

impl Drop for Scheduler<'_> {
    fn drop(&mut self) {
        if self.owner {
            self.end();
        }
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

/// The future of [`Scheduler::wait`].
#[must_use = "a wait parks the task only when the task awaits it"]
pub struct Wait<'a, 'c> {
    scheduler: &'a Scheduler<'c>,
    /// The condition, until the wait ends or parks the task.
    condition: Option<Condition<'c>>,
}

impl fmt::Debug for Wait<'_, '_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Wait")
            .field("pending", &self.condition.is_some())
            .finish_non_exhaustive()
    }
}

impl Future for Wait<'_, '_> {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<()> {
        let Some(condition) = self.condition.take() else {
            return Poll::Ready(());
        };
        if condition() {
            return Poll::Ready(());
        }
        let mut state = self.scheduler.locked();
        assert!(state.turn, "{WAIT_OUTSIDE}");
        state.parking = Some(Park::Wait(condition));
        Poll::Pending
    }
}

/// The clock of a [`Scheduler`], which [`Scheduler::clock`] returns.
///
/// [`Clock::now`] returns the time since the Unix epoch. The clock starts at
/// midnight UTC 2000-01-01, and only [`Scheduler::advance`] moves it. A task
/// sleeps through the future of [`TaskClock::sleep`].
pub struct TaskClock<'c> {
    shared: Arc<Shared<'c>>,
}

impl fmt::Debug for TaskClock<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TaskClock")
            .field("now", &self.now())
            .finish()
    }
}

impl<'c> TaskClock<'c> {
    /// Returns a future that parks the running task until the time of the
    /// clock is its deadline or later. The deadline is the time of the call
    /// plus `duration`. A sleep of zero is ready at once.
    ///
    /// # Panics
    ///
    /// Panics outside a task for a positive `duration`.
    pub fn sleep(&self, duration: Duration) -> Sleep<'c> {
        let state = self.shared.locked();
        assert!(duration.is_zero() || state.turn, "{SLEEP_OUTSIDE}");
        let deadline = (!duration.is_zero()).then(|| state.now.saturating_add(duration));
        drop(state);
        Sleep {
            shared: Arc::clone(&self.shared),
            deadline,
        }
    }
}

impl Clock for TaskClock<'_> {
    fn now(&self) -> Duration {
        self.shared.locked().now
    }

    /// Returns at once for a `duration` of zero.
    ///
    /// # Panics
    ///
    /// Panics for a positive `duration`. Outside a task only the body can
    /// advance the clock, and inside a task a blocking sleep stops every
    /// task.
    fn sleep(&self, duration: Duration) {
        assert!(duration.is_zero(), "{SLEEP_OUTSIDE}");
    }
}

/// The future of [`TaskClock::sleep`].
#[must_use = "a sleep parks the task only when the task awaits it"]
pub struct Sleep<'c> {
    shared: Arc<Shared<'c>>,
    /// The deadline, until the sleep parks the task, and `None` for a sleep
    /// of zero.
    deadline: Option<Duration>,
}

impl fmt::Debug for Sleep<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Sleep")
            .field("deadline", &self.deadline)
            .finish_non_exhaustive()
    }
}

impl Future for Sleep<'_> {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<()> {
        let Some(deadline) = self.deadline.take() else {
            return Poll::Ready(());
        };
        let mut state = self.shared.locked();
        assert!(state.turn, "{SLEEP_OUTSIDE}");
        state.parking = Some(Park::Sleep(deadline));
        Poll::Pending
    }
}

impl<'c> Scheduler<'c> {
    /// Returns a scheduler that releases the tasks of `case` by `strategy`,
    /// with the limit of releases of the strategy.
    pub fn new(case: &'c Case<'_>, strategy: Strategy) -> Self {
        let state = State {
            now: EPOCH,
            ..State::default()
        };
        Self {
            shared: Arc::new(Shared {
                case,
                strategy,
                state: Mutex::new(state),
            }),
            owner: true,
        }
    }

    fn locked(&self) -> MutexGuard<'_, State<'c>> {
        self.shared.locked()
    }

    /// Returns an integer choice of the case, and ends the case when it
    /// cannot make one.
    fn integer(&self, bounds: IntegerBounds, edge: Option<i128>) -> i128 {
        let case = self.shared.case;
        match case.integer(bounds, edge, false) {
            Ok(value) => value,
            Err(stop) => case.end(stop),
        }
    }

    /// Returns the priority of a task under PCT, a choice of the case, and 0
    /// under the uniform strategy.
    fn priority(&self) -> i128 {
        if self.shared.strategy.depth == 0 {
            return 0;
        }
        self.integer(IntegerBounds::of(0, UINT64_MAX), None)
    }

    /// Returns the number of the next task in the order of spawning.
    fn number(&self) -> usize {
        let mut state = self.locked();
        state.count += 1;
        state.count - 1
    }

    /// Makes a task ready after every ready task. Under PCT, the task first
    /// receives its priority, a choice of the case. The task does not run
    /// until a release starts it. The release calls `task` with a handle of
    /// the scheduler, and polls the future that `task` returns. The task
    /// awaits [`Scheduler::yield_now`], [`Scheduler::wait`] and the sleep of
    /// [`Scheduler::clock`] through the handle. A handle does not end a task
    /// when it drops.
    pub fn spawn<F>(&self, task: impl FnOnce(Scheduler<'c>) -> F + Send + 'c)
    where
        F: Future<Output = ()> + Send + 'c,
    {
        let priority = self.priority();
        let handle = Scheduler {
            shared: Arc::clone(&self.shared),
            owner: false,
        };
        let future: Task<'c> = Box::pin(async move { task(handle).await });
        let number = self.number();
        self.locked().spawned.push(held(number, future, priority));
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

    /// Returns a future that parks the running task until `condition`
    /// returns true. A poll of the future calls `condition`, and the future
    /// is ready when it returns true. Otherwise the task parks. The scheduler
    /// then calls `condition` between two turns without the lock of its
    /// state, and makes the task ready when `condition` returns true. A
    /// condition that panics ends the call of the scheduler as a task that
    /// panics does. Outside a task, `wait` calls `condition` at once, and the
    /// future is ready.
    ///
    /// # Panics
    ///
    /// Panics outside a task for a `condition` that returns false.
    pub fn wait(&self, condition: impl Fn() -> bool + Send + Sync + 'c) -> Wait<'_, 'c> {
        let condition: Condition<'c> = Arc::new(condition);
        if !self.locked().turn {
            assert!(condition(), "{WAIT_OUTSIDE}");
            return Wait {
                scheduler: self,
                condition: None,
            };
        }
        Wait {
            scheduler: self,
            condition: Some(condition),
        }
    }

    /// Returns the clock of the scheduler, at midnight UTC 2000-01-01 until
    /// [`Scheduler::advance`] moves it.
    #[must_use]
    pub fn clock(&self) -> TaskClock<'c> {
        TaskClock {
            shared: Arc::clone(&self.shared),
        }
    }

    /// Makes the change points of PCT, and then releases one ready task at a
    /// time until none is ready. A task may spawn others while it runs. The
    /// run does not move the clock, so a sleeping task remains parked until
    /// an advance. A task that panics ends the run: the run ends every task
    /// and raises the panic again on the calling thread. A run that has
    /// released the limit of its strategy while a task is still ready ends
    /// every task, and fails the case with the message "the task scheduler
    /// released n tasks in one run, and a task is still ready".
    ///
    /// # Panics
    ///
    /// Panics for a call through the handle of a task, and raises the panic
    /// of a task or a condition.
    pub fn run(&self) {
        self.execute("run()", Vec::new(), None);
    }

    /// Releases the ready tasks as [`Scheduler::run`] does, and moves the
    /// clock by `duration`, deadline by deadline. Once no task is ready, the
    /// advance moves the clock to the earliest deadline of a sleeping task,
    /// or to its end time when no deadline is earlier. The tasks of that
    /// deadline then run at its time, so a task that sleeps again measures
    /// its next deadline from there. The advance returns when the time of the
    /// clock is its end time and no task is ready. Under PCT, the change
    /// points count the releases of the whole advance from 0.
    ///
    /// # Panics
    ///
    /// Panics as [`Scheduler::run`] panics.
    pub fn advance(&self, duration: Duration) {
        self.execute("advance()", Vec::new(), Some(duration));
    }

    /// Returns the counts of the change points of a call: up to `depth − 1`
    /// under PCT, and none under the uniform strategy.
    fn change_points(&self) -> Vec<usize> {
        let mut points = Vec::new();
        for _ in 1..self.shared.strategy.depth {
            if self.integer(IntegerBounds::of(0, 1), Some(1)) == 1 {
                let count = self.integer(IntegerBounds::of(0, UINT64_MAX), None);
                points.push(usize::try_from(count).unwrap_or(usize::MAX));
            }
        }
        points
    }

    /// Removes the task to release from the ready tasks and returns it: under
    /// PCT the task of the highest level and then the highest priority, the
    /// earliest ready among equals, and otherwise the case's choice among the
    /// ready tasks. A call that has released the limit of its strategy does
    /// not release another task.
    fn next<'m>(
        &self,
        ready: &mut VecDeque<Held<Work<'c, 'm>>>,
        released: usize,
    ) -> Result<Option<Held<Work<'c, 'm>>>, Halt> {
        if ready.is_empty() {
            return Ok(None);
        }
        if released == self.shared.strategy.max_releases {
            return Err(Halt::Limit);
        }
        let index = if self.shared.strategy.depth > 0 {
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
            let case = self.shared.case;
            match case.choose(request) {
                Ok(choice) => {
                    usize::try_from(choice.as_integer().unwrap_or_default()).unwrap_or_default()
                }
                Err(stop) => case.end(stop),
            }
        };
        Ok(ready.remove(index))
    }

    /// Runs one turn of `task`: polls it once, and returns how the turn
    /// ended, or the payload of its panic.
    fn turn(&self, task: &mut Held<Work<'c, '_>>) -> std::thread::Result<Turn<'c>> {
        {
            let mut state = self.locked();
            state.turn = true;
            state.yielded = false;
        }
        task.woken.0.store(false, Ordering::SeqCst);
        let waker = Waker::from(Arc::clone(&task.woken));
        let mut context = Context::from_waker(&waker);
        let polled = catch_unwind(AssertUnwindSafe(|| task.work.poll(&mut context)));
        let mut state = self.locked();
        state.turn = false;
        let (parking, yielded) = (state.parking.take(), state.yielded);
        drop(state);
        polled.map(|poll| match (poll, parking) {
            (Poll::Ready(()), _) => Turn::Returned,
            (Poll::Pending, Some(park)) => Turn::Parked(park),
            (Poll::Pending, None) if yielded => Turn::Yielded,
            (Poll::Pending, None) => Turn::Waiting,
        })
    }

    /// Runs one call of the scheduler: the release of the tasks of the
    /// state and of `clients`, and for an advance the moves of the clock by
    /// `advance`.
    fn execute(&self, call: &str, clients: Vec<Local<'_>>, advance: Option<Duration>) {
        let mut queue = self.start(call, clients);
        match self.release(&mut queue, advance) {
            Ok(()) => self.stop(queue),
            Err(halt) => {
                drop(queue);
                self.end();
                match halt {
                    Halt::Panicked(payload) => resume_unwind(payload),
                    Halt::Limit => self.shared.case.fail(&format!(
                        "the task scheduler released {} tasks in one run, and a task is still ready",
                        self.shared.strategy.max_releases
                    )),
                }
            }
        }
    }

    /// Starts a call. The call takes the parked tasks out of the state and
    /// makes ready the tasks spawned since the last turn. Then it makes
    /// `clients` ready, each with its priority under PCT.
    fn start<'m>(&self, call: &str, clients: Vec<Local<'m>>) -> Queue<'c, 'm> {
        let parked = {
            let mut state = self.locked();
            assert!(
                self.owner && !state.running,
                "stateful: a task called {call} of its own scheduler"
            );
            state.running = true;
            take(&mut state.parked)
        };
        let mut queue = Queue {
            ready: VecDeque::new(),
            parked: parked
                .into_iter()
                .map(|(task, park)| (task.into_work(), park))
                .collect(),
            waiting: Vec::new(),
        };
        self.admit(&mut queue.ready);
        for client in clients {
            let priority = self.priority();
            let number = self.number();
            queue
                .ready
                .push_back(held(number, Work::Client(client), priority));
        }
        queue
    }

    /// Releases the tasks of a call, and moves the clock of an advance by
    /// `advance`, deadline by deadline, to its end time.
    fn release(&self, queue: &mut Queue<'c, '_>, advance: Option<Duration>) -> Result<(), Halt> {
        let end = advance.map(|duration| self.locked().now.saturating_add(duration));
        self.wake(queue)?;
        let points = self.change_points();
        let mut released = 0;
        loop {
            released = self.release_ready(queue, &points, released)?;
            let Some(end) = end else {
                return Ok(());
            };
            let now = queue.earliest(end);
            self.locked().now = now;
            self.wake(queue)?;
            if queue.ready.is_empty() && now == end {
                return Ok(());
            }
        }
    }

    /// Releases one ready task at a time until none is ready, and returns
    /// the count of the releases of the call, which `released` starts.
    fn release_ready(
        &self,
        queue: &mut Queue<'c, '_>,
        points: &[usize],
        mut released: usize,
    ) -> Result<usize, Halt> {
        while let Some(mut task) = self.next(&mut queue.ready, released)? {
            for _ in points.iter().filter(|point| **point == released) {
                let mut state = self.locked();
                state.lowest -= 1;
                task.level = state.lowest;
            }
            released += 1;
            let turn = self.turn(&mut task);
            self.admit(&mut queue.ready);
            match turn.map_err(Halt::Panicked)? {
                Turn::Yielded => queue.ready.push_back(task),
                Turn::Returned => {}
                Turn::Parked(park) => queue.parked.push((task, park)),
                Turn::Waiting => queue.waiting.push(task),
            }
            let (woken, still): (Vec<_>, Vec<_>) = take(&mut queue.waiting)
                .into_iter()
                .partition(|task| task.woken.0.load(Ordering::SeqCst));
            queue.ready.extend(woken);
            queue.waiting = still;
            self.wake(queue)?;
        }
        Ok(released)
    }

    /// Makes ready, after every ready task, each sleeping task whose deadline
    /// is the time of the clock or earlier, by deadline and then in the order
    /// of the sleeps, and then each waiting task whose condition returns
    /// true, in the order of the waits.
    fn wake(&self, queue: &mut Queue<'c, '_>) -> Result<(), Halt> {
        let now = self.locked().now;
        let mut due = Vec::new();
        let mut met = Vec::new();
        for (task, park) in take(&mut queue.parked) {
            let woken = match &park {
                Park::Sleep(deadline) => *deadline <= now,
                Park::Wait(condition) => {
                    catch_unwind(AssertUnwindSafe(&**condition)).map_err(Halt::Panicked)?
                }
            };
            match (woken, park) {
                (true, Park::Sleep(deadline)) => due.push((deadline, task)),
                (true, Park::Wait(_)) => met.push(task),
                (false, park) => queue.parked.push((task, park)),
            }
        }
        due.sort_by_key(|(deadline, _)| *deadline);
        queue.ready.extend(due.into_iter().map(|(_, task)| task));
        queue.ready.extend(met);
        Ok(())
    }

    /// Makes ready the tasks spawned since the last turn, in the order of
    /// spawning.
    fn admit<'m>(&self, ready: &mut VecDeque<Held<Work<'c, 'm>>>) {
        let spawned = take(&mut self.locked().spawned);
        ready.extend(spawned.into_iter().map(Held::into_work));
    }

    /// Ends a call that returned without a halt. The parked tasks of the
    /// scheduler remain parked for the next call, and the parked clients of a
    /// section end. A task that awaits a future of something other than the
    /// scheduler ends the case with a fault.
    fn stop(&self, queue: Queue<'c, '_>) {
        let parked: Vec<(Held<Task<'c>>, Park<'c>)> = queue
            .parked
            .into_iter()
            .filter_map(|(task, park)| Some((task.into_task()?, park)))
            .collect();
        {
            let mut state = self.locked();
            state.running = false;
            state.parked = parked;
        }
        if !queue.waiting.is_empty() {
            let numbers: Vec<String> = queue
                .waiting
                .iter()
                .map(|task| task.number.to_string())
                .collect();
            self.shared.case.end(Stop::Fault(format!(
                "stateful: the tasks {} wait on a future that no task of the scheduler wakes",
                numbers.join(", ")
            )));
        }
    }

    /// Ends every task that has not returned: none of them runs again.
    fn end(&self) {
        let ended = {
            let mut state = self.locked();
            state.running = false;
            (take(&mut state.spawned), take(&mut state.parked))
        };
        drop(ended);
    }
}

/// Returns a task of the scheduler at level 0.
fn held<W>(number: usize, work: W, priority: i128) -> Held<W> {
    Held {
        number,
        work,
        priority,
        level: 0,
        woken: Arc::new(Woken(AtomicBool::new(false))),
    }
}

/// A scheduler that runs the clients of a concurrent section.
pub(crate) trait Section {
    /// Runs `tasks`, one per client, as tasks of the scheduler. A client that
    /// waits or sleeps when no task is ready ends with the section.
    fn run_section(&self, tasks: Vec<Local<'_>>);
}

impl Section for Scheduler<'_> {
    fn run_section(&self, tasks: Vec<Local<'_>>) {
        self.execute("run()", tasks, None);
    }
}
