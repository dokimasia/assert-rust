//! Assertions whose subject is a future.
//!
//! The synchronous crate covers thirty-four of the standard's assertions, and a Rust
//! caller uses it for all of them. This adds the ones whose subject cannot be a
//! synchronous closure.
#![allow(clippy::missing_panics_doc)]

pub mod check;
