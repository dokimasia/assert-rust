// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The definition's corpus and vectors, run against `dokimi-assert` and
//! `dokimi-assert-tokio` through their public APIs.
//!
//! The crate is not published. Its specs under `tests/` run every case and
//! every vector of the vendored definition beside its sources, which
//! `make spec-sync` refreshes with spec.
//!
//! The drivers decode the definition's typed literals with a decoder of
//! their own, and compare JSON through `serde_json`. They never compare
//! through the library under test, because a defect of the library could
//! pass a check that the library makes.

pub mod corpus;
pub mod definition;
pub mod files;
pub mod history;
pub mod literal;
pub mod prop;
pub mod stateful;
pub mod subject;
pub mod vector;
