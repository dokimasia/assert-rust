// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The property engine: choices, cases, the runner, the shrinker and the
//! store.
//!
//! The engine implements the definition's algorithms: every language that
//! implements them decodes the same values from the same seed, shrinks a
//! failure to the same case and names its store entry the same. The
//! conformance crate runs the definition's vectors against these modules,
//! which are no part of the public API and change without notice.

pub mod ahead;
pub mod bridge;
pub(crate) mod campaign;
pub mod canonical;
pub mod case;
pub mod choice;
pub mod collection;
pub mod coverage;
pub mod draw;
pub mod edge;
pub mod execution;
pub mod harness;
pub mod inverse;
pub mod runner;
pub mod shrink;
pub mod source;
pub mod store;
pub mod token;
pub mod trace;
pub mod tree;
