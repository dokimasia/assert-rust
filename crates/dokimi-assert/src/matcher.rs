//! The comparison of each family of assertions.
//!
//! Each matcher takes a [`Mode`](crate::verdict::Mode) that states whether a
//! failure ends the test. [`check`](crate::check) and [`soft`](crate::soft)
//! call the same matchers with the mode fixed, so the two surfaces agree by
//! construction.

pub(crate) mod allocations;
pub(crate) mod behaviour;
pub(crate) mod body;
pub(crate) mod containment;
pub(crate) mod errors;
pub(crate) mod numbers;
pub(crate) mod order;
pub(crate) mod panics;
pub(crate) mod rejects;
pub(crate) mod relation;
pub(crate) mod sizes;
pub(crate) mod text;
pub(crate) mod values;
pub(crate) mod waiting;
