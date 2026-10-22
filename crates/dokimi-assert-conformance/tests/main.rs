// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The specs of the conformance drivers, in one test binary.
//!
//! Each source file under `src/` has its spec at the same path under
//! `tests/`. The spec of `corpus` runs every case of the definition's corpus
//! on both surfaces of each library that states its assertion.

mod corpus;
mod definition;
mod files;
mod history;
mod literal;
mod prop;
mod stateful;
mod subject;
mod vector;
