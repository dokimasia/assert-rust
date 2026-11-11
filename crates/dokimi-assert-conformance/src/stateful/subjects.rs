// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The machine subjects of the definition, which a machines vector names.
//!
//! Every subject records each call of a client in the case's history,
//! without keys, and completes it as ok with its output:
//!
//! - `queue-loses-on-wrap`: a bounded queue of a capacity drawn under
//!   `capacity` from [1, 8]. Its put writes the value at the write index and
//!   returns true, or returns false when the queue is full. Its get returns
//!   the oldest value, or null when the queue is empty. When the write index
//!   is at the last slot, put writes the value into the first slot, and the
//!   index wraps to the first slot. The actions are put, whose input is drawn
//!   under `v` from [0, 1000], and get. The spec is a bounded queue of the
//!   capacity.
//! - `correct-queue`: the same machine over a queue that writes every value
//!   at its write index.
//! - `counter-overflows`: a counter whose increment adds one and returns the
//!   count, and whose reset sets the count to 0 and returns null. When an
//!   increment takes the count to 3, the counter sets it to 0 and returns 0.
//!   The actions are increment and reset, and the spec is a counter.
//! - `store-loses-on-crash`: a store that puts a value under a key into a
//!   buffer, moves the buffer into its durable map on flush, and empties the
//!   buffer on crash. The actions are put, flush and crash. The input of put
//!   is a key drawn under `key` from [0, 3] and the next value of a count
//!   from 1. Flush is a drain action, enabled while the buffer is not empty.
//!   The machine has no spec. Settle reads the durable value of every key
//!   that a put returned for. It fails with the record of `lost-write` when
//!   the value is not the last one put.
//! - `racy-counter`: a counter whose increment reads the count, yields to the
//!   task scheduler, writes the count plus one and returns it. The action is
//!   increment, the spec a counter, and the clients run as tasks of a
//!   scheduler.
//! - `correct-counter`: the same machine over an increment that adds one and
//!   returns the count without a yield.
//! - `counter-refuses-every-third`: a counter that refuses every third
//!   increment, counted over every case of the run. A refused increment
//!   completes as failed with the error `refused`, and leaves the count. Any
//!   other increment adds one and returns the count, and read returns the
//!   count. The actions are increment, enabled while the spec's count is
//!   below 3, and read. The spec is a counter.
//! - `register-flushes-in-two-parts`: a register of a buffer and a durable
//!   value, both initially null. Its write sets the buffer to the value and
//!   returns null. Its read returns the buffer when it is set, and the
//!   durable value otherwise. A flusher task, which the subject spawns before
//!   the steps, waits until the buffer is set, takes the value and sets the
//!   buffer to null, yields, sets the durable value to the value that it
//!   took, and waits again. The actions are write, whose input is drawn under
//!   `v` from [0, 1000], and read. The spec is a register, and the clients of
//!   the concurrent section run as tasks of the scheduler of the flusher.
//! - `register-flushes`: the same machine over a flusher that sets the
//!   durable value and the buffer without a yield between.
//! - `ticker-skips-its-first-tick`: a ticker task, which the subject spawns
//!   before the steps, sleeps 4 ms on the clock of its scheduler and then
//!   adds one to its count, four times. It skips the addition of its first
//!   tick. The actions are advance and read. Advance advances the scheduler
//!   by its input in milliseconds, drawn under `ms` from [0, 10], and returns
//!   null. Read returns the count. The spec counts the milliseconds of the
//!   advances, and its read returns the milliseconds divided by 4, rounded
//!   down and at most 4.
//! - `ticker`: the same machine over a ticker that adds one at each tick.
//! - `spins-until-a-flag`: a flag, initially false, that no action sets. The
//!   action is spin, which spawns a task that yields until the flag is set,
//!   runs the scheduler, and does not record a call. The machine has no
//!   spec, so a spin fails its case at the limit of releases of the
//!   scheduler.
//!
//! [`subject`] builds each subject anew for a run, so a subject that counts
//! over the cases of a run starts each run at 0.

use dokimi_assert::Value;
use dokimi_assert::history::{Operation, Spec};
use dokimi_assert::prop::{self, Case};
use dokimi_assert::stateful::{self, Action, Machine, Options, Scheduler, Strategy};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

/// The count at which `counter-overflows` sets its count to 0.
const OVERFLOW: i64 = 3;

/// The interval of the increments that `counter-refuses-every-third`
/// refuses, counted over a run, and the error of a refusal.
const REFUSAL: u64 = 3;
const REFUSED: &str = "refused";

/// The count of the spec below which `counter-refuses-every-third` enables
/// its increment.
const LIMIT: i64 = 3;

/// The assertion and the contract of the record that the settle of
/// `store-loses-on-crash` reports.
const LOST: &str = "lost-write";
const DURABLE: &str = "the store keeps every acknowledged put";

/// The milliseconds that the ticker sleeps before each of its ticks, and its
/// ticks.
const PERIOD: u64 = 4;
const TICKS: u64 = 4;

/// The options of a subject's steps, those of a concurrent section, and the
/// strategy of a subject's scheduler.
#[derive(Debug, Clone, Copy)]
pub struct Setup {
    /// The options that every subject takes: mean, max, swarm and the mean
    /// steps of a concurrent section.
    pub steps: Options<'static>,
    /// The clients of a concurrent section.
    pub clients: usize,
    /// The most steps of a concurrent section, when the setup states it.
    pub concurrent: Option<usize>,
    /// The strategy of the scheduler that a subject with a concurrent section
    /// makes in each case.
    pub strategy: Strategy,
}

/// A subject: it builds its subject and runs its machine in a case.
pub type Subject = Box<dyn Fn(&Case<'_>, &Setup) + Send + Sync>;

/// Returns the subject named `name` for one run, or `None` for a name that
/// the definition does not state.
#[must_use]
pub fn subject(name: &str) -> Option<Subject> {
    let subject: Subject = match name {
        "queue-loses-on-wrap" => Box::new(|c: &Case<'_>, setup: &Setup| queue(c, setup, true)),
        "correct-queue" => Box::new(|c: &Case<'_>, setup: &Setup| queue(c, setup, false)),
        "counter-overflows" => Box::new(counter_overflows),
        "store-loses-on-crash" => Box::new(store_loses_on_crash),
        "racy-counter" => Box::new(|c: &Case<'_>, setup: &Setup| shared_counter(c, setup, true)),
        "correct-counter" => {
            Box::new(|c: &Case<'_>, setup: &Setup| shared_counter(c, setup, false))
        }
        "counter-refuses-every-third" => refusing_counter(),
        "register-flushes-in-two-parts" => {
            Box::new(|c: &Case<'_>, setup: &Setup| flushed_register(c, setup, true))
        }
        "register-flushes" => {
            Box::new(|c: &Case<'_>, setup: &Setup| flushed_register(c, setup, false))
        }
        "ticker-skips-its-first-tick" => {
            Box::new(|c: &Case<'_>, setup: &Setup| ticker(c, setup, true))
        }
        "ticker" => Box::new(|c: &Case<'_>, setup: &Setup| ticker(c, setup, false)),
        "spins-until-a-flag" => Box::new(spinner),
        _ => return None,
    };
    Some(subject)
}

/// Takes a lock. A step that panicked while it held the lock left a valid
/// state, so the lock's poison is ignored.
fn locked<T>(lock: &Mutex<T>) -> MutexGuard<'_, T> {
    lock.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A bounded queue in a ring of slots. A ring that loses on wrap writes the
/// value of a put at the last slot into the first slot.
struct Ring {
    slots: Vec<Option<i64>>,
    loses_on_wrap: bool,
    head: usize,
    tail: usize,
    size: usize,
}

impl Ring {
    fn new(capacity: usize, loses_on_wrap: bool) -> Self {
        Self {
            slots: vec![None; capacity],
            loses_on_wrap,
            head: 0,
            tail: 0,
            size: 0,
        }
    }

    /// Adds `value`, and returns whether the ring had room for it.
    fn put(&mut self, value: i64) -> bool {
        let capacity = self.slots.len();
        if self.size == capacity {
            return false;
        }
        let last = self.tail == capacity - 1;
        let at = if last && self.loses_on_wrap {
            0
        } else {
            self.tail
        };
        self.slots[at] = Some(value);
        self.tail = if last { 0 } else { self.tail + 1 };
        self.size += 1;
        true
    }

    /// Removes and returns the value of the oldest slot. An empty ring and a
    /// slot that no put wrote return `None`.
    fn get(&mut self) -> Option<i64> {
        if self.size == 0 {
            return None;
        }
        let value = self.slots[self.head];
        self.head = (self.head + 1) % self.slots.len();
        self.size -= 1;
        value
    }
}

/// The spec of a queue of at most `capacity` values: put appends its value
/// while the queue has room, and get returns the oldest value and removes
/// it. A get leaves an empty queue empty. The spec states the calls of the
/// queue subjects, whose put reports the room that the spec counts, and
/// whose get of an empty queue returns null.
struct BoundedQueue {
    capacity: usize,
}

impl Spec for BoundedQueue {
    type State = Vec<i64>;

    fn initial(&self) -> Vec<i64> {
        Vec::new()
    }

    fn next(&self, state: &Vec<i64>, op: &Operation) -> Vec<Vec<i64>> {
        if op.name() == "put" {
            let mut after = state.clone();
            if let (true, [Value::Int(value)]) = (state.len() < self.capacity, op.args()) {
                after.push(i64::try_from(*value).unwrap_or_default());
            }
            return vec![after];
        }
        match state.split_first() {
            None => vec![state.clone()],
            Some((head, rest)) if op.returned(*head) => vec![rest.to_vec()],
            Some(_) => Vec::new(),
        }
    }
}

/// The spec of a counter: increment returns the new count, reset sets the
/// count to 0, and read returns the count.
struct Counter;

impl Spec for Counter {
    type State = i64;

    fn initial(&self) -> i64 {
        0
    }

    fn next(&self, state: &i64, op: &Operation) -> Vec<i64> {
        if op.name() == "reset" {
            return vec![0];
        }
        let after = if op.name() == "read" {
            *state
        } else {
            state + 1
        };
        if op.returned(after) {
            vec![after]
        } else {
            Vec::new()
        }
    }
}

/// The spec of a register, initially null: write leaves its value, and read
/// returns the value.
struct Register;

impl Spec for Register {
    type State = Option<i64>;

    fn initial(&self) -> Option<i64> {
        None
    }

    fn next(&self, state: &Option<i64>, op: &Operation) -> Vec<Option<i64>> {
        if let ("write", [Value::Int(value)]) = (op.name(), op.args()) {
            return vec![i64::try_from(*value).ok()];
        }
        if op.returned(state.map_or(Value::Null, Value::from)) {
            vec![*state]
        } else {
            Vec::new()
        }
    }
}

/// The spec of the ticker, whose state is the milliseconds that the advances
/// add. Its read returns the milliseconds divided by the period, rounded down
/// and at most the ticks.
struct Ticked;

impl Spec for Ticked {
    type State = u64;

    fn initial(&self) -> u64 {
        0
    }

    fn next(&self, state: &u64, op: &Operation) -> Vec<u64> {
        if let ("advance", [Value::Int(ms)]) = (op.name(), op.args()) {
            return vec![state + u64::try_from(*ms).unwrap_or_default()];
        }
        if op.returned((state / PERIOD).min(TICKS)) {
            vec![*state]
        } else {
            Vec::new()
        }
    }
}

/// Returns the options of a subject whose clients of the concurrent section
/// run as tasks of `scheduler`.
fn on_tasks<'s>(setup: &Setup, scheduler: &'s Scheduler<'_>) -> Options<'s> {
    let mut options = setup.steps.clients(setup.clients);
    if let Some(most) = setup.concurrent {
        options = options.concurrent(most);
    }
    options.tasks(scheduler)
}

/// Runs the queue machine over a ring of a drawn capacity.
fn queue(c: &Case<'_>, setup: &Setup, loses_on_wrap: bool) {
    let capacity = c.draw(&prop::integer(1..=8_usize), "capacity");
    let ring = Mutex::new(Ring::new(capacity, loses_on_wrap));
    let machine = Machine::new([
        Action::with_input(
            "put",
            |c: &Case<'_>, _: &Vec<i64>| c.draw(&prop::integer(0..=1000_i64), "v"),
            async |c: &Case<'_>, client: usize, value: i64| {
                let call = c.history().invoke(client, "put", [Value::from(value)], []);
                call.ok(locked(&ring).put(value));
            },
        ),
        Action::new("get", async |c: &Case<'_>, client: usize| {
            let call = c.history().invoke(client, "get", [], []);
            call.ok(locked(&ring).get().map_or(Value::Null, Value::from));
        }),
    ])
    .spec(BoundedQueue { capacity });
    stateful::steps(c, &machine, setup.steps);
}

/// Runs the counter machine over a counter that overflows at [`OVERFLOW`].
fn counter_overflows(c: &Case<'_>, setup: &Setup) {
    let count = AtomicI64::new(0);
    let machine = Machine::new([
        Action::new("increment", async |c: &Case<'_>, client: usize| {
            let call = c.history().invoke(client, "increment", [], []);
            let after = (count.load(Ordering::SeqCst) + 1) % OVERFLOW;
            count.store(after, Ordering::SeqCst);
            call.ok(after);
        }),
        Action::new("reset", async |c: &Case<'_>, client: usize| {
            let call = c.history().invoke(client, "reset", [], []);
            count.store(0, Ordering::SeqCst);
            call.ok(Value::Null);
        }),
    ])
    .spec(Counter);
    stateful::steps(c, &machine, setup.steps);
}

/// Runs the store machine, whose settle reads every acknowledged put.
fn store_loses_on_crash(c: &Case<'_>, setup: &Setup) {
    let durable: Mutex<BTreeMap<i64, i64>> = Mutex::default();
    let buffer: Mutex<BTreeMap<i64, i64>> = Mutex::default();
    let acknowledged: Mutex<BTreeMap<i64, i64>> = Mutex::default();
    let written = AtomicI64::new(0);
    let machine = Machine::new([
        Action::with_input(
            "put",
            |c: &Case<'_>, (): &()| {
                let key = c.draw(&prop::integer(0..=3_i64), "key");
                (key, written.fetch_add(1, Ordering::SeqCst) + 1)
            },
            async |c: &Case<'_>, client: usize, (key, stored): (i64, i64)| {
                let call =
                    c.history()
                        .invoke(client, "put", [Value::from(key), Value::from(stored)], []);
                locked(&buffer).insert(key, stored);
                call.ok(Value::Null);
                locked(&acknowledged).insert(key, stored);
            },
        ),
        Action::new("flush", async |_: &Case<'_>, _: usize| {
            let flushed = std::mem::take(&mut *locked(&buffer));
            locked(&durable).extend(flushed);
        })
        .enabled(|(): &()| !locked(&buffer).is_empty())
        .drain(),
        Action::new("crash", async |_: &Case<'_>, _: usize| {
            locked(&buffer).clear();
        }),
    ])
    .settle(|c: &Case<'_>, (): &()| {
        let kept = locked(&durable).clone();
        let lost = locked(&acknowledged)
            .iter()
            .find(|(key, stored)| kept.get(key) != Some(stored))
            .map(|(key, stored)| (*key, *stored));
        if let Some((key, stored)) = lost {
            c.fail_with(
                LOST,
                DURABLE,
                vec![("key", Value::from(key)), ("lost", Value::from(stored))],
            );
        }
    });
    stateful::steps(c, &machine, setup.steps);
}

/// Runs the counter machine on clients that run as tasks, over an increment
/// that yields between its read and its write when `racy`.
fn shared_counter(c: &Case<'_>, setup: &Setup, racy: bool) {
    let scheduler = Scheduler::new(c, setup.strategy);
    let count = AtomicI64::new(0);
    let machine = Machine::new([Action::new(
        "increment",
        async |c: &Case<'_>, client: usize| {
            let call = c.history().invoke(client, "increment", [], []);
            let read = count.load(Ordering::SeqCst);
            if racy {
                scheduler.yield_now().await;
            }
            count.store(read + 1, Ordering::SeqCst);
            call.ok(read + 1);
        },
    )])
    .spec(Counter);
    stateful::steps(c, &machine, on_tasks(setup, &scheduler));
}

/// Runs the register machine on clients that run as tasks, over a buffer
/// that a flusher task moves into the durable value. The flusher yields
/// between its two parts when `in_two_parts`.
fn flushed_register(c: &Case<'_>, setup: &Setup, in_two_parts: bool) {
    let register: Mutex<(Option<i64>, Option<i64>)> = Mutex::default();
    let scheduler = Scheduler::new(c, setup.strategy);
    let flushed = &register;
    scheduler.spawn(move |s| async move {
        loop {
            s.wait(move || locked(flushed).0.is_some()).await;
            let taken = locked(flushed).0.take();
            if in_two_parts {
                s.yield_now().await;
            }
            locked(flushed).1 = taken;
        }
    });
    let machine = Machine::new([
        Action::with_input(
            "write",
            |c: &Case<'_>, _: &Option<i64>| c.draw(&prop::integer(0..=1000_i64), "v"),
            async |c: &Case<'_>, client: usize, value: i64| {
                let call = c
                    .history()
                    .invoke(client, "write", [Value::from(value)], []);
                locked(&register).0 = Some(value);
                call.ok(Value::Null);
            },
        ),
        Action::new("read", async |c: &Case<'_>, client: usize| {
            let call = c.history().invoke(client, "read", [], []);
            let (buffer, durable) = *locked(&register);
            call.ok(buffer.or(durable).map_or(Value::Null, Value::from));
        }),
    ])
    .spec(Register);
    stateful::steps(c, &machine, on_tasks(setup, &scheduler));
}

/// Runs the ticker machine, whose advance moves the clock of the scheduler
/// of the ticker task. The ticker skips the addition of its first tick when
/// `skips_first`.
fn ticker(c: &Case<'_>, setup: &Setup, skips_first: bool) {
    let count = AtomicU64::new(0);
    let scheduler = Scheduler::new(c, setup.strategy);
    let ticked = &count;
    scheduler.spawn(move |s| async move {
        for tick in 0..TICKS {
            s.clock().sleep(Duration::from_millis(PERIOD)).await;
            if tick > 0 || !skips_first {
                ticked.fetch_add(1, Ordering::SeqCst);
            }
        }
    });
    let machine = Machine::new([
        Action::with_input(
            "advance",
            |c: &Case<'_>, _: &u64| c.draw(&prop::integer(0..=10_u64), "ms"),
            async |c: &Case<'_>, client: usize, ms: u64| {
                let call = c.history().invoke(client, "advance", [Value::from(ms)], []);
                scheduler.advance(Duration::from_millis(ms));
                call.ok(Value::Null);
            },
        ),
        Action::new("read", async |c: &Case<'_>, client: usize| {
            c.history()
                .invoke(client, "read", [], [])
                .ok(count.load(Ordering::SeqCst));
        }),
    ])
    .spec(Ticked);
    stateful::steps(c, &machine, setup.steps);
}

/// Runs the machine whose one action spawns a task that yields until a flag
/// is set, and runs the scheduler. No action sets the flag.
fn spinner(c: &Case<'_>, setup: &Setup) {
    let flag = AtomicBool::new(false);
    let scheduler = Scheduler::new(c, setup.strategy);
    let set = &flag;
    let machine = Machine::<()>::new([Action::new("spin", async |_: &Case<'_>, _: usize| {
        scheduler.spawn(move |s| async move {
            while !set.load(Ordering::SeqCst) {
                s.yield_now().await;
            }
        });
        scheduler.run();
    })]);
    stateful::steps(c, &machine, setup.steps);
}

/// Returns `counter-refuses-every-third`, with no increment counted yet.
fn refusing_counter() -> Subject {
    let increments = AtomicU64::new(0);
    Box::new(move |c: &Case<'_>, setup: &Setup| {
        let count = AtomicI64::new(0);
        let machine = Machine::new([
            Action::new("increment", async |c: &Case<'_>, client: usize| {
                let call = c.history().invoke(client, "increment", [], []);
                if (increments.fetch_add(1, Ordering::SeqCst) + 1) % REFUSAL == 0 {
                    call.fail(REFUSED);
                    return;
                }
                call.ok(count.fetch_add(1, Ordering::SeqCst) + 1);
            })
            .enabled(|state: &i64| *state < LIMIT),
            Action::new("read", async |c: &Case<'_>, client: usize| {
                c.history()
                    .invoke(client, "read", [], [])
                    .ok(count.load(Ordering::SeqCst));
            }),
        ])
        .spec(Counter);
        stateful::steps(c, &machine, setup.steps);
    })
}
