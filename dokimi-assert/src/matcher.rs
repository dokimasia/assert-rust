//! The comparisons both surfaces report through.
//!
//! Each family is a module, and each takes a [`Mode`] saying whether a
//! failure stops the test or is recorded. [`check`](crate::check) and
//! [`soft`](crate::soft) are the same calls with the mode fixed, which is what makes the
//! two surfaces agree by construction rather than by review.

pub mod behaviour;
pub mod containment;
pub mod errors;
pub mod numbers;
pub mod order;
pub mod panics;
pub mod report;
pub mod sizes;
pub mod text;
pub mod values;
pub mod waiting;

pub use report::{Mode, report};
pub use sizes::Container;
