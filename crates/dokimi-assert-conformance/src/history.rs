// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The history vectors of the definition, run against the history checks of
//! `dokimi-assert`.
//!
//! A seam vector states a script of calls or a log of intervals and the
//! events that it records. A linearizable vector states a script and a named
//! spec, and an isolation vector a script of list-append transactions. Each
//! check vector states the verdict and the detail of the call record.

pub mod specs;
pub mod vectors;
