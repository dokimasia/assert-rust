// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! Test assertions, properties and history checks, defined by a
//! language-neutral standard.
//!
//! The assertions mean the same in every language of the standard: Go,
//! Python, TypeScript, Java, Kotlin and Rust run one corpus of cases and one
//! set of vectors, so a Rust test and its Go port check the same thing.
//!
//! ```
//! use dokimi_assert::{Collector, check, soft};
//!
//! let t = Collector::new();
//! let reply = (200, String::from("{\"items\":[1,2,3]}"));
//! check::equal(&t, reply.0, 200, "the request succeeds");
//! soft::has_prefix(&t, &reply.1, "{", "the body is a JSON object");
//! soft::contains(&t, reply.1.as_str(), "items", "the body lists the items");
//! ```
//!
//! # Two surfaces
//!
//! [`check`] ends the test at the first failure. [`soft`] records a failure
//! and lets the test continue, so one run reports every failure. Both take
//! the same assertions under the same names.
//!
//! # Seats
//!
//! Every assertion reports to the [`Seat`] that the test hands it. A test
//! creates a [`Collector`]: its failures on `check` panic, and the failures
//! on `soft` that it collected panic when it drops. A test of an assertion
//! hands it a [`Recorder`] and reads the records.
//!
//! A call that refuses an argument or its environment ends with a fault,
//! which the seat receives through [`Seat::report_fault`]. A helper of a
//! test ends its own call the same way with [`fault`].
//! [`allocs_checked`] returns whether the test binary checks allocation
//! ceilings.
//!
//! # The other modules
//!
//! - [`prop`] runs a body against generated inputs and shrinks a failure.
//! - [`history`] checks recorded histories for linearizability and isolation.
//! - [`stateful`] drives a machine through generated steps.
//! - [`files`] builds and compares trees of files.
//! - [`golden`] compares output with recorded files.
//! - [`bench`](mod@bench) states performance ceilings.

mod alloc;
mod alphabet;
mod cancel;
mod clock;
mod failure;
mod json;
mod matcher;
mod pattern;
mod record;
mod seat;
mod value;
mod verdict;

pub mod bench;
pub mod check;
pub mod files;
pub mod golden;
pub mod history;
pub mod prop;
pub mod soft;
pub mod stateful;

pub use alloc::allocs_checked;
pub use cancel::{Cancel, Stop, Stopped};
pub use clock::{Clock, Controlled, System};
pub use failure::{Failure, Where};
pub use matcher::containment::Contains;
pub use matcher::numbers::Number;
pub use matcher::sizes::Container;
pub use seat::{Call, Collector, Recorder, Seat, Standard, fault};
pub use value::Value;

/// The items that `dokimi-assert-tokio` builds its assertions from. They are
/// no part of the public API, and they change without notice.
#[doc(hidden)]
pub mod __private {
    pub use crate::matcher::behaviour::{nanoseconds, waited};
    pub use crate::matcher::body::Body;
    pub use crate::matcher::errors::chain;
    pub use crate::matcher::panics::{message, quietly};
    pub use crate::prop::registry::{
        convert_from, convert_to, record_fields, unknown_variant, variant_parts,
    };
    pub use crate::record::Calls;
    pub use crate::verdict::{Mode, Run};
}
