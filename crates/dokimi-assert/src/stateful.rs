// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The steps of a machine inside a property's case: the sequential
//! specification of a subject, actions over it, and the task scheduler that
//! releases the tasks of a subject in an order that the case decides.
//!
//! A [`Machine`] lists its actions in order, the simpler first, and states
//! the sequential specification of the subject. [`steps`] takes the steps of
//! a case, and records every decision as a choice of the case, so a failing
//! case shrinks to the steps that the failure needs and replays from its
//! token:
//!
//! ```
//! use dokimi_assert::history::{Operation, Spec};
//! use dokimi_assert::stateful::{self, Action, Machine};
//! use dokimi_assert::{Collector, Value, prop};
//! use std::collections::VecDeque;
//! use std::sync::Mutex;
//!
//! /// A queue: put adds a value at the tail, get returns the head or null.
//! struct Queue;
//!
//! impl Spec for Queue {
//!     type State = Vec<i64>;
//!
//!     fn initial(&self) -> Vec<i64> {
//!         Vec::new()
//!     }
//!
//!     fn next(&self, state: &Vec<i64>, op: &Operation) -> Vec<Vec<i64>> {
//!         match (op.name(), op.args()) {
//!             ("put", [Value::Int(v)]) => vec![[state.as_slice(), &[*v as i64]].concat()],
//!             ("get", []) => match state.split_first() {
//!                 Some((head, rest)) if op.returned(*head) => vec![rest.to_vec()],
//!                 None if op.returned(Value::Null) => vec![state.clone()],
//!                 _ => Vec::new(),
//!             },
//!             _ => Vec::new(),
//!         }
//!     }
//! }
//!
//! let t = Collector::new();
//! prop::for_all(&t, "the queue keeps its values in order", prop::cases(20), |c| {
//!     let queue = Mutex::new(VecDeque::new());
//!     let machine = Machine::new([
//!         Action::with_input(
//!             "put",
//!             |c, _| c.draw(&prop::integer(0..=9_i64), "v"),
//!             async |c, client, v| {
//!                 let call = c.history().invoke(client, "put", [v.into()], []);
//!                 queue.lock().unwrap().push_back(v);
//!                 call.ok(Value::Null);
//!             },
//!         ),
//!         Action::new("get", async |c, client| {
//!             let call = c.history().invoke(client, "get", [], []);
//!             let head = queue.lock().unwrap().pop_front();
//!             call.ok(head.map_or(Value::Null, Value::from));
//!         }),
//!     ])
//!     .spec(Queue);
//!     stateful::steps(c, &machine, stateful::max(8));
//! });
//! ```
//!
//! A concurrent section runs its clients on threads, and a case that runs one
//! repeats, because threads do not replay. Under [`tasks`], the clients run
//! as tasks of a [`Scheduler`], whose releases are choices of the case, so a
//! race replays and shrinks. A subject that runs as tasks awaits
//! [`Scheduler::yield_now`] at each point where another task may run.

mod machine;
mod options;
mod scheduler;
mod steps;
mod strategy;

pub use machine::{Action, Machine};
pub use options::{Options, clients, concurrent, max, mean, repeat, swarm, tasks};
pub use scheduler::{Scheduler, YieldNow};
pub use steps::steps;
pub use strategy::{Strategy, pct, uniform};
