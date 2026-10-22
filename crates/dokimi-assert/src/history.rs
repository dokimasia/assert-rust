// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! Histories: the calls that concurrent clients make to an object, and the
//! checks that a recorded history is linearizable, serializable or has
//! snapshot isolation.
//!
//! A test records each call through a [`History`]: [`History::invoke`]
//! before the call to the subject starts, and [`Call::ok`], [`Call::fail`] or
//! [`Call::unknown`] after it returns. [`concurrently`] runs the clients, and
//! [`is_linearizable`] checks the history against a [`Spec`], the object's
//! sequential specification:
//!
//! ```
//! use dokimi_assert::history::{self, History, Operation, Spec};
//! use dokimi_assert::{Collector, Value};
//! use std::sync::{Arc, Mutex};
//! use std::time::Duration;
//!
//! struct Register;
//!
//! impl Spec for Register {
//!     type State = Value;
//!
//!     fn initial(&self) -> Value {
//!         Value::Int(0)
//!     }
//!
//!     fn next(&self, state: &Value, op: &Operation) -> Vec<Value> {
//!         match op.name() {
//!             "write" => op.args().to_vec(),
//!             _ if op.returned(state.clone()) => vec![state.clone()],
//!             _ => Vec::new(),
//!         }
//!     }
//! }
//!
//! let h = Arc::new(History::new());
//! let register = Arc::new(Mutex::new(0_i64));
//! let (recorded, subject) = (Arc::clone(&h), Arc::clone(&register));
//! history::concurrently(2, Duration::from_secs(60), move |client| {
//!     for i in 0..20 {
//!         let value = i64::try_from(client * 1000 + i).unwrap_or_default();
//!         let write = recorded.invoke(client, "write", [value.into()], ["x".into()]);
//!         *subject.lock().unwrap() = value;
//!         write.ok(Value::Null);
//!         let read = recorded.invoke(client, "read", [], ["x".into()]);
//!         let got = *subject.lock().unwrap();
//!         read.ok(got);
//!     }
//!     Ok::<(), ()>(())
//! });
//! let t = Collector::new();
//! history::is_linearizable(&t, &h, &Register, "the register is linearizable", Default::default());
//! ```
//!
//! The definition fixes the recording order, the processes, the partitions,
//! the search, its budget and its record, so one history gives the same
//! verdict and the same record in every implementation of the definition.
//! [`spec_from`] builds the spec from the subject itself, which checks that
//! the calls were atomic. [`is_serializable`] and [`has_snapshot_isolation`]
//! read each call of the operation `txn` as a transaction of list appends
//! and list reads.

mod concurrently;
mod cycle;
mod derivation;
mod interval;
mod isolation;
pub(crate) mod linearizable;
mod options;
mod partition;
mod recording;
mod search;
mod sentence;
mod spec;
mod transaction;

pub use concurrently::{Outcome, concurrently};
pub use interval::{Ending, Interval, IntervalError, from_intervals};
pub use isolation::{has_snapshot_isolation, is_serializable};
pub use linearizable::is_linearizable;
pub use options::{Options, budget, memo_limit, time_limit, workers};
pub use recording::{Call, Event, History, Kind};
pub use spec::{Operation, Spec, spec_from};
