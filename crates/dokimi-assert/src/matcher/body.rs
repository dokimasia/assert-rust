// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The seat that an assertion makes for a body: an attempt of `eventually`,
//! or the check of `rejects`.
//!
//! The calls of the body are a run of the call that ran it, so their records
//! go under that call. The body reads the clock of the assertion's seat, and
//! its cancellation handle derives from the handle of the assertion's seat
//! and stops when the body ends. A failure on `check` ends the body, as it
//! ends a test. A fault that ends its call ends the body with the text of the
//! fault, and a fault that a call notes passes to the assertion's seat. A
//! skip ends the check of `rejects`, and an attempt of `eventually` runs on
//! past it.

use crate::cancel::Cancel;
use crate::clock::Clock;
use crate::failure::Failure;
use crate::record::Calls;
use crate::seat::{self, Seat};
use std::error::Error;
use std::fmt;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

/// The payload with which the seat of a body ends the body at a failure on
/// `check` and at a skip of a check. [`Body::finish`] swallows it, and
/// `resume_unwind` raises it without the panic hook, so the end prints
/// nothing.
struct Ended;

/// The seat of one run of a body.
pub struct Body<'p> {
    parent: &'p dyn Seat,
    cancellation: Cancel,
    calls: Option<Arc<Calls>>,
    /// Whether the body is the check of `rejects`, which a skip ends.
    check: bool,
    state: Mutex<State>,
}

#[derive(Debug, Default)]
struct State {
    failed: bool,
    first: Option<String>,
    failures: Vec<Failure>,
    fault: Option<Arc<dyn Error + Send + Sync>>,
    skip: Option<String>,
}

impl fmt::Debug for Body<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Body")
            .field("cancellation", &self.cancellation)
            .field("state", &*self.locked())
            .finish_non_exhaustive()
    }
}

impl<'p> Body<'p> {
    /// Returns the seat of a run of a body of an assertion on `parent`. The
    /// calls of the body go to `calls`. A skip writes its reason as the
    /// default of [`Seat::skip`] does, and the body runs on.
    pub fn new(parent: &'p dyn Seat, calls: Option<Arc<Calls>>) -> Self {
        Self::of(parent, calls, false)
    }

    /// Returns the seat of the check of `rejects`, as [`Body::new`] returns
    /// the seat of a body. A skip keeps the first reason and ends the check.
    pub(crate) fn check(parent: &'p dyn Seat, calls: Option<Arc<Calls>>) -> Self {
        Self::of(parent, calls, true)
    }

    fn of(parent: &'p dyn Seat, calls: Option<Arc<Calls>>, check: bool) -> Self {
        Self {
            parent,
            cancellation: parent.cancellation().child(),
            calls,
            check,
            state: Mutex::default(),
        }
    }

    fn locked(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Runs `body` on the seat, and ends the run as [`Body::finish`] states.
    pub fn run(&self, body: impl FnOnce(&dyn Seat)) {
        self.finish(catch_unwind(AssertUnwindSafe(|| body(self))));
    }

    /// Ends a run of a body whose unwinding `outcome` states, and stops the
    /// seat's handle.
    ///
    /// A failure on `check` ends the body without a panic of the assertion.
    /// Any other panic of the body is no failure of the body, so it panics
    /// again past the assertion.
    pub fn finish(&self, outcome: std::thread::Result<()>) {
        self.cancellation.cancel();
        if let Err(payload) = outcome {
            if !payload.is::<Ended>() {
                resume_unwind(payload);
            }
        }
    }

    /// Returns `true` when any failure was reported to the seat.
    #[must_use]
    pub fn failed(&self) -> bool {
        self.locked().failed
    }

    /// Returns the sentence of the first failure, and the empty string when
    /// nothing failed.
    #[must_use]
    pub fn first_message(&self) -> String {
        self.locked().first.clone().unwrap_or_default()
    }

    /// Returns the records of the failures, in the order of the calls.
    #[must_use]
    pub fn failures(&self) -> Vec<Failure> {
        self.locked().failures.clone()
    }

    /// Returns the first fault that ended a call of the body, or `None` when
    /// no fault did.
    #[must_use]
    pub fn fault(&self) -> Option<Arc<dyn Error + Send + Sync>> {
        self.locked().fault.clone()
    }

    /// Returns the reason of the first skip of the check, or `None` when the
    /// check did not skip.
    pub(crate) fn skipped(&self) -> Option<String> {
        self.locked().skip.clone()
    }

    fn note(&self, message: &str) {
        let mut state = self.locked();
        state.failed = true;
        if state.first.is_none() {
            state.first = Some(message.to_owned());
        }
    }
}

impl Seat for Body<'_> {
    fn fail(&self, message: &str) {
        self.note(message);
        resume_unwind(Box::new(Ended));
    }

    fn record(&self, message: &str) {
        self.note(message);
    }

    fn report(&self, failure: &Failure, aborting: bool) {
        self.locked().failures.push(failure.clone());
        self.note(&failure.to_string());
        if aborting {
            resume_unwind(Box::new(Ended));
        }
    }

    /// Ends the body at a fault that ends its call, and keeps the first such
    /// fault. A noted fault passes to the seat of the assertion, and the body
    /// continues.
    fn report_fault(&self, fault: Arc<dyn Error + Send + Sync>, ending: bool) {
        if !ending {
            self.parent.report_fault(fault, false);
            return;
        }
        let text = fault.to_string();
        self.locked().fault.get_or_insert(fault);
        self.fail(&text);
    }

    /// Ends the check of `rejects` at a skip, and keeps the first reason. An
    /// attempt of `eventually` writes the reason as the default does, and
    /// runs on.
    fn skip(&self, reason: &str) {
        if !self.check {
            seat::write_skip(reason);
            return;
        }
        self.locked().skip.get_or_insert_with(|| reason.to_owned());
        resume_unwind(Box::new(Ended));
    }

    fn clock(&self) -> &dyn Clock {
        self.parent.clock()
    }

    fn cancellation(&self) -> &Cancel {
        &self.cancellation
    }

    fn calls(&self) -> Option<Arc<Calls>> {
        self.calls.clone()
    }
}
