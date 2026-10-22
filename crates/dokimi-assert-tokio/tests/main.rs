// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The specs of `dokimi-assert-tokio`, in one test binary.
//!
//! Each source file under `src/` has its spec at the same path under
//! `tests/`. A spec that waits runs on a runtime whose clock starts paused,
//! so the wait does not spend real time.

mod check;
mod helpers;
mod matcher;
mod soft;
