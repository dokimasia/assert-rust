---
rfc: 0005
title: Definition 16.0.0 in Rust
author: Roy Klopper <roy.klopper@stealthscale.io>
status: Accepted
created: 2026-11-11
updated: 2026-11-11
discussion: none
supersedes: RFC-0003, in part
superseded-by: none
produces-adr: none
---

<!--
  ~ Copyright Dokimasia B.V. 2026
  ~ SPDX-License-Identifier: MIT
-->

# RFC-0005: Definition 16.0.0 in Rust

## Summary

The library moves from definition 15.0.0 to 16.0.0.

| Change | Rust API |
|---|---|
| A run replays a failing case n times before the shrink | `prop::replays`, `Options::replays`, `RunResult::replays` and `RunResult::reproduced` |
| A task parks until a condition is true | `Scheduler::wait` |
| A task sleeps on the clock of its scheduler | `Scheduler::clock` and `TaskClock` |
| The body moves the clock of the scheduler | `Scheduler::advance` |
| A run or an advance fails at a limit of releases | `stateful::max_releases` and `Strategy::max_releases` |
| A parked task remains parked after the run that parked it returns | `Scheduler::spawn` gives a task a handle of its scheduler |

The last row changes the API of RFC-0003: a task receives a handle of its
scheduler instead of a borrow, and its future is `Send`. Every call record
states 16.0.0.

## Motivation

The completeness gate fails on every row of the naming table that the
library neither names nor declines. Definition 16.0.0 adds the rows
`prop.replays`, `run-result.replays`, `run-result.reproduced`,
`scheduler.wait`, `scheduler.clock`, `scheduler.advance` and
`scheduler.max-releases`, and it pins them with six machines vectors and
four property vectors.

The design of the scheduler follows from these facts of the current code:

- RFC-0003 makes the future of each task when a run starts the task, and the
  future borrows the scheduler for that run. The definition parks a task
  across calls: the ticker of `ticker-skips-its-first-tick` sleeps 4 ms
  before each of its four ticks, and each tick happens in a later call of
  `advance`. A struct cannot keep a future that borrows the struct, and the
  crate denies `unsafe_code`.
- `Clock::sleep` blocks its thread. A task runs on the thread of `run`, so a
  blocking sleep stops every task, and the body that would advance the clock
  waits for `run` to return. A future parks only when its poll returns
  `Poll::Pending`.
- `Case::cleanup` takes a `'static` closure, so a scheduler of a case cannot
  end its tasks through a cleanup of the case.

## Detailed design

### The replays of a failing case

```rust
impl<T> Options<T> {
    /// States how often a run replays a failing case before it shrinks the
    /// case, once by default.
    ///
    /// # Panics
    ///
    /// Panics for `n` of 0.
    pub fn replays(self, n: usize) -> Self;
}

pub fn replays<T>(n: usize) -> Options<T>;

pub struct RunResult {
    // The fields of definition 15.0.0, then:
    /// The replays of the failing case before the shrink.
    pub replays: Option<usize>,
    /// The replays that reproduced the failure.
    pub reproduced: Option<usize>,
}
```

- `conclude` of `runner.rs` replays the failing case `n` times, and every
  replay runs. A replay reproduces the failure when the comparison of the
  replay of a failing case does not find a difference. The run shrinks the case
  when one or more replays reproduce the failure. It ends as flaky with the
  difference of the first replay when no replay does.
- The detail of a run gains the fields `replays` and `reproduced`, after
  `reason`, so it has thirteen fields. A run that replays no failing case
  states null for both: a pass, a run of a replay token, a shrink budget of
  0 and a failing example of values.
- `RunResult` is `#[non_exhaustive]`, so its two fields do not break a
  caller.
- The panic message of `replays(0)` is `prop: replays(0) replays no failing
  case`.

### A task that outlives its run

```rust
impl<'c> Scheduler<'c> {
    pub fn new(case: &'c Case<'_>, strategy: Strategy) -> Self;
    pub fn spawn<F>(&self, task: impl FnOnce(Scheduler<'c>) -> F + Send + 'c)
    where
        F: Future<Output = ()> + Send + 'c;
    pub fn yield_now(&self) -> YieldNow<'_, 'c>;
    pub fn wait(&self, condition: impl Fn() -> bool + Send + Sync + 'c) -> Wait<'_, 'c>;
    pub fn clock(&self) -> TaskClock<'c>;
    pub fn run(&self);
    pub fn advance(&self, duration: Duration);
}
```

A scheduler is a handle of one shared state behind an `Arc`. The state
contains the tasks that have not returned: the ready tasks, the parked
tasks, and the tasks that a call of `spawn` added since the last turn.

- `spawn` takes a function that returns the future of the task. A run calls
  the function when it starts the task, with a handle of the scheduler. The
  future then borrows no scheduler, so the state keeps it between calls of
  `run` and `advance`.
- The future of a task is `Send`. The state is behind a `Mutex`, and
  `Options::tasks` stores the scheduler as `&(dyn Section + Sync)`. A
  `Mutex<T>` is `Sync` only for a `T` that is `Send`. Stable Rust cannot
  bound the future of an `AsyncFnOnce` closure by `Send`, because its type
  `CallOnceFuture` is unstable, so `spawn` takes `FnOnce(..) -> F`.
- The scheduler that `new` returns ends every task that has not returned
  when it drops. A handle that a task receives ends none, and a task cannot
  call `run` or `advance` through it. The body drops the scheduler before
  its case ends, so a parked task ends with its case and does not return
  from its wait or its sleep.
- A task that awaits a future of something other than the scheduler waits
  until another task wakes it. When no task is ready and such a task waits
  at the end of a call, the call ends the case with the fault of RFC-0003.

A task of the doc example of RFC-0003 changes its form and keeps its body:

```rust
scheduler.spawn(move |s| async move {
    steps.lock().unwrap().push(name);
    s.yield_now().await;
    steps.lock().unwrap().push(name);
});
```

### Wait

`wait` returns a future. A poll of the future inside a task calls the
condition, and returns `Poll::Ready` when it returns true. Otherwise the
future moves the condition into the state of the running task, and the task
parks. The scheduler calls the condition of each waiting task between two
turns, in the order of the waits, and makes the task ready when it returns
true. The next poll of the future then returns `Poll::Ready` without a call
of the condition.

- The condition is an `Arc<dyn Fn() -> bool + Send + Sync + 'c>`. The
  scheduler takes a clone of it out of the lock and calls the clone without
  the lock, so a condition can read the clock of its scheduler.
- Outside a task, `wait` calls the condition at once. It returns a future
  that is ready when the condition returns true, and it panics when the
  condition returns false.

### The clock of the scheduler

```rust
pub struct TaskClock<'c> { /* private fields */ }

impl Clock for TaskClock<'_> {
    fn now(&self) -> Duration;
    fn sleep(&self, duration: Duration);
}

impl<'c> TaskClock<'c> {
    pub fn sleep(&self, duration: Duration) -> Sleep<'c>;
}
```

- `now` returns the time since the Unix epoch. The clock starts at
  946,684,800 s, midnight UTC 2000-01-01, and only `Scheduler::advance`
  moves it. `Clock::advance` keeps its default, which does nothing.
- The inherent `sleep` returns a future. Inside a task, a sleep of a
  positive duration parks the task until the clock reads its deadline: the
  time at the sleep plus the duration. A sleep of zero returns at once. A
  method call on a `TaskClock` resolves to the inherent `sleep`, so a task
  writes `s.clock().sleep(d).await`.
- `Clock::sleep` of a `TaskClock` returns at once for a duration of zero,
  and panics for a positive duration. Outside a task, only the body can
  advance the clock. Inside a task, a blocking sleep would stop every task.

### Advance

`advance` takes the change points of a run under PCT and counts its
releases from 0 over all its steps. It releases the ready tasks as `run`
does. It then moves the clock to the earliest deadline of a parked sleep,
or to the end time when no deadline is due before it, and makes ready the
tasks that the move makes due. It returns when the clock reads the end time
and no task is ready. A duration of zero releases tasks as `run` does.

### The release limit

```rust
impl Strategy {
    /// States the most tasks that one run or one advance releases.
    ///
    /// # Panics
    ///
    /// Panics for `n` of 0.
    pub fn max_releases(self, n: usize) -> Self;
}

/// Returns the uniform strategy with a limit of `n` releases.
pub fn max_releases(n: usize) -> Strategy;
```

`new` takes the limit from its strategy, which is 1,000,000 by default. The
free function starts a chain from the uniform strategy, as each free
function of `stateful::Options` starts its chain. A run or an advance that
has released the limit while a task is still ready ends every task, and
fails the case through `Seat::fail` with the message of the definition.

### The concurrent section of a machine

The futures of the clients of a section borrow the machine and the case for
the call of `steps`. The body creates the machine after the scheduler,
because an action such as `advance` of the ticker borrows the scheduler. So
the state of the scheduler cannot keep a future of a client.

- The section runs its client futures and the tasks of the state in one
  ready queue, which keeps both kinds apart by their lifetimes.
- When the run of the section returns, a client that waits or sleeps ends
  there, and its call remains open. The check after the section reads the
  open call as a call without a completion.
- The definition lets a drain action complete such a call, and a client of
  Rust cannot resume after its section. The overlay of Rust then needs a
  limit that states this, which is a change of assert-spec.

### Invariants

- A task runs only inside `run` or `advance`, on the thread of the call.
- A parked task is either sleeping or waiting, and it is not ready.
- The tasks that one check makes ready join after every ready task: the
  sleeps by deadline and then in the order of the sleeps, then the waits in
  their order. A task that yields joins before them.
- The clock never moves backwards, and only `advance` moves it.
- One run or one advance releases at most the limit of its strategy.
- The scheduler does not call a condition while it has the lock of its
  state.

### Failures

| Event | Result |
|---|---|
| A task panics | The call ends every task and raises the panic on its thread, as in RFC-0003 |
| A condition panics | The same as a task that panics |
| `wait` outside a task with a condition that returns false | Panics: `stateful: a wait outside a task waits for a condition that is false` |
| `Clock::sleep` of a positive duration, or the inherent `sleep` outside a task | Panics: `stateful: a sleep outside a task waits for a clock that only the body moves` |
| A task calls `run` or `advance` of its scheduler | Panics: `stateful: a task called run() of its own scheduler`, or `advance()` |
| A call releases the limit while a task is ready | Ends every task and fails the case: `the task scheduler released n tasks in one run, and a task is still ready` |
| `max_releases(0)` | Panics: `stateful: max_releases(0) is no count of 1 or more` |

### Version

`DEFINITION` of `record.rs` becomes `16.0.0`. The signature of `spawn`, the
`Send` bound of a task's future and the field of `Strategy` break the API,
so the changesets of the scheduler are breaking.

## Alternatives considered

### A. End every task at the end of each call

`spawn` would keep its signature, and a task that is parked when a call
returns would end there.

**Why not:** the ticker vectors fail, because each tick of the ticker
happens in a later call of `advance`. A flusher that waits for a write of a
later step never sees the write.

### B. A scheduler that keeps futures that borrow it

The state would keep the future of each task with the borrow of the
scheduler, through a raw pointer and `Pin`.

**Why not:** the crate denies `unsafe_code`, and a scheduler that moves
after a task borrows it would leave a dangling borrow.

### C. Tasks of `'static`, as `tokio::spawn` requires them

**Why not:** a task borrows the state of the subject that the body creates,
such as the buffer of a flusher. A `'static` task would need every state of
a subject behind an `Arc`.

### D. Tasks whose futures are not `Send`

The state would keep the futures outside the `Mutex`, and the scheduler
would not be `Sync`.

**Why not:** `Options::tasks` stores `&(dyn Section + Sync)`, and the
options of `steps` cross the threads of a section that runs on threads.
Each subject would need a second options type for tasks.

### E. A third parameter of `new` for the release limit

`new` would take an options type of the scheduler, as Go's `NewScheduler`
takes `MaxReleases`.

**Why not:** every call of `new` breaks, and the options type has one
setting. The strategy is already the setting of the scheduler that `new`
takes.

### F. The clients of a section as tasks of the state

**Why not:** the future of a client borrows the machine, and the scheduler
outlives the machine. The drop check then refuses the scheduler.

## Drawbacks

- Every task of a caller changes from `async move |s| { … }` to
  `move |s| async move { … }`.
- The future of a task is `Send`, so a task cannot keep an `Rc` or a
  `MutexGuard` across an `.await`.
- A state that a task borrows must be declared before the scheduler, so the
  scheduler drops first.
- The scheduler calls the condition of every waiting task after each turn,
  so each release costs one call for each waiting task.
- A client of a section that waits or sleeps at the end of the section does
  not resume, which departs from the definition until its overlay states the
  limit.
- No vector covers the panics of `wait`, `sleep`, `advance` and
  `max_releases`, so the specs of the library alone test them.

## References

| What | Where |
|---|---|
| Tasks that wait and sleep under the task scheduler | <https://github.com/dokimasia/assert-spec/blob/main/docs/rfc/0056-tasks-that-wait-and-sleep.md> |
| The replays of a failing case | <https://github.com/dokimasia/assert-spec/blob/main/docs/rfc/0057-the-replays-of-a-failing-case.md> |
| The task scheduler of Rust | [RFC-0003](0003-definition-9-in-rust.md) |
| The traits of async closures | <https://doc.rust-lang.org/std/ops/trait.AsyncFnOnce.html> |
| `tokio::time::sleep` | <https://docs.rs/tokio/latest/tokio/time/fn.sleep.html> |
