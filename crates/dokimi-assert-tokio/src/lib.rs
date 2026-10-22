// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The assertions of `dokimi-assert` whose subject is a future, for tests
//! that run on tokio.
//!
//! Each assertion has the name of its synchronous counterpart in
//! `dokimi-assert` and returns a future that the test awaits. A subject that
//! can be cancelled takes the same [`Cancel`] handle as a synchronous
//! subject, and awaits [`Cancel::stopped`] to learn that it should stop.
//!
//! ```
//! use dokimi_assert::{Cancel, Recorder, Stop};
//! use dokimi_assert_tokio::check;
//! use std::time::Duration;
//!
//! /// Fetches a page, or returns the reason of the handle when it stops first.
//! async fn fetch(cancel: Option<&Cancel>) -> Result<String, Stop> {
//!     let Some(cancel) = cancel else {
//!         return Ok("fetched".to_owned());
//!     };
//!     tokio::select! {
//!         reason = cancel.stopped() => Err(reason),
//!         () = tokio::time::sleep(Duration::from_secs(60)) => Ok("fetched".to_owned()),
//!     }
//! }
//!
//! # #[tokio::main(flavor = "current_thread")]
//! # async fn main() {
//! let t = Recorder::new();
//! check::honours_cancellation(&t, async |cancel| fetch(cancel).await.map(drop), "the fetch stops").await;
//! assert!(!t.failed(), "{}", t.message());
//! # }
//! ```
//!
//! # Bodies
//!
//! An assertion takes an async closure when it hands the body an argument,
//! runs the body more than once, or reads the runtime before the body
//! starts. [`check::completes_within`] only times its body, so it takes a
//! future, as `tokio::time::timeout` does.
//!
//! # Two surfaces
//!
//! [`check`] ends the test at the first failure, and [`soft`] records a
//! failure and lets the test continue, as the surfaces of `dokimi-assert` do.
//!
//! # Time
//!
//! The assertions that wait read tokio's clock, so they run inside a tokio
//! runtime with its time driver. A test that pauses the clock, as
//! `#[tokio::test(start_paused = true)]` does, does not spend real time on a
//! retry or a timeout. `honours_cancellation` and `honours_deadline` do not
//! read a clock, and they run on any executor.
//!
//! [`Cancel`]: dokimi_assert::Cancel
//! [`Cancel::stopped`]: dokimi_assert::Cancel::stopped

pub mod check;
pub mod soft;

mod matcher;
