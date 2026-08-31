//! Test assertions defined by a language-neutral standard.
//!
//! Two surfaces carry the same assertions under the same names. [`check`] stops the test
//! at the first failure; [`soft`] records and carries on, so one run shows every property
//! that failed.
//!
//! ```
//! use dokimi_assert::{check, seat::Collector};
//!
//! let seat = Collector::new();
//! check::equal(&seat, &(1 + 1), &2, "addition works");
//! ```
//!
//! Every assertion takes a seat first and a message last. The message states the contract
//! under test and is the first line of the failure, so a failure says what was supposed to
//! be true rather than only what was observed.
//!
//! # Equality
//!
//! Comparison is the language's own `==`, which is already what the standard asks for:
//! `f64::NAN` is unequal to itself, `0.0` equals `-0.0`, and containers compare by their
//! elements. Values of different types do not compare because they do not compile.
//!
//! # Where a failure goes
//!
//! [`seat::Seat`] is the trait an assertion reports through. Which seat a test holds
//! decides what each surface does; [`seat::Collector`] is the one a real test wants.

pub mod bench;
pub mod check;
pub mod clock;
pub mod failure;
pub mod golden;
pub mod matcher;
pub mod seat;
pub mod soft;

pub use seat::{Collector, Recorder, Seat, Standard};
