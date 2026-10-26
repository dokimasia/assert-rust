// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The verdict of every assertion call: a pass, a failure, a skip or a
//! fault.
//!
//! Each verdict writes the call's record when its seat records calls. A
//! failure then sends its record to the seat, and a skip passes its reason to
//! the seat's `skip`. A fault ends the call through the seat's `report_fault`
//! on either surface, because the call checked nothing and a test that ran on
//! would meet the fault again.

use crate::failure::{Failure, Where};
use crate::json::Json;
use crate::record::{self, Call, Calls, Slot, Switch, Verdict};
use crate::seat::Seat;
use crate::value::Value;
use std::error::Error;
use std::fmt::{self, Display};
use std::panic::Location;
use std::sync::Arc;

/// A fault of this library: the text of an argument or an environment that a
/// call refuses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Fault(String);

impl Fault {
    /// Returns the fault of `text`.
    pub(crate) fn new(text: impl Display) -> Self {
        Self(text.to_string())
    }
}

impl Display for Fault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Error for Fault {}

/// Ends a call on `seat` with `fault` through [`Seat::report_fault`].
#[track_caller]
pub(crate) fn end(seat: &dyn Seat, fault: impl Display) {
    seat.report_fault(Arc::new(Fault::new(fault)), true);
}

/// Notes `fault` on `seat` through [`Seat::report_fault`], and runs on.
#[track_caller]
pub(crate) fn note(seat: &dyn Seat, fault: impl Display) {
    seat.report_fault(Arc::new(Fault::new(fault)), false);
}

/// Whether a failure ends the test or lets it continue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// The failure ends the test: the surface `check`.
    Fatal,
    /// The test continues past the failure: the surface `soft`.
    Soft,
}

/// One call of an assertion, which reports its verdict once.
///
/// A call reads its call site when it starts, so a matcher that reports from
/// a closure still names the caller's line.
pub struct Run<'s> {
    seat: &'s dyn Seat,
    slot: Option<Slot>,
    location: Where,
}

impl fmt::Debug for Run<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Run")
            .field("location", &self.location)
            .finish_non_exhaustive()
    }
}

impl<'s> Run<'s> {
    /// Returns a call on `seat` that runs no body.
    #[track_caller]
    pub fn of(seat: &'s dyn Seat) -> Self {
        Self {
            seat,
            slot: None,
            location: Where::from(Location::caller()),
        }
    }

    /// Starts a call on `seat` that runs a body. A recorded call takes its
    /// number now, before the calls of its body.
    #[track_caller]
    pub fn begin(seat: &'s dyn Seat) -> Self {
        Self::begin_at(seat, Where::from(Location::caller()))
    }

    /// Starts a call on `seat` that runs a body, at the call site `location`.
    pub(crate) fn begin_at(seat: &'s dyn Seat, location: Where) -> Self {
        let slot = switched_on()
            .then(|| seat.calls())
            .flatten()
            .map(|calls| Slot::begin(&calls));
        Self {
            seat,
            slot,
            location,
        }
    }

    /// Returns the calls of one run of the call's body: calls whose records
    /// the call takes when the run ends, and `None` for a call that is not
    /// recorded.
    #[must_use]
    pub fn body_calls(&self) -> Option<Arc<Calls>> {
        self.slot.as_ref().map(|_| Calls::running(None))
    }

    /// Takes the calls of one run of the body, whose calls `body` kept, under
    /// the call.
    pub fn take(&self, body: Option<&Arc<Calls>>) {
        self.take_in_phase(body, None);
    }

    /// Takes the calls of one run of the body as [`Run::take`] does, each
    /// with `phase`, the kind of a property's case that the run ran.
    pub(crate) fn take_in_phase(&self, body: Option<&Arc<Calls>>, phase: Option<record::Phase>) {
        if let (Some(slot), Some(body)) = (&self.slot, body) {
            slot.take(body, phase);
        }
    }

    /// Returns the call site of the call.
    pub(crate) fn location(&self) -> Where {
        self.location
    }

    /// Returns `true` when the call is recorded, so that its body records its
    /// calls.
    pub(crate) fn recorded(&self) -> bool {
        self.slot.is_some()
    }

    /// Reports that a property passed, with the detail of its run, which its
    /// call record states.
    pub(crate) fn pass_run(
        &self,
        mode: Mode,
        assertion: &'static str,
        contract: &str,
        detail: Json,
    ) {
        if !self.switched() {
            return;
        }
        self.write(Call {
            assertion,
            contract: contract.to_owned(),
            verdict: Verdict::Pass,
            aborting: mode == Mode::Fatal,
            location: Some(self.location),
            detail: Some(detail),
            error: None,
            reason: None,
        });
    }

    /// Reports that the call ended as skipped with `reason`. A property
    /// reports the detail of its run, which its call record states, and
    /// `rejects` reports no detail. The seat then receives the skip through
    /// [`Seat::skip`], which may not return.
    pub(crate) fn skip_run(
        &self,
        mode: Mode,
        assertion: &'static str,
        contract: &str,
        detail: Option<Json>,
        reason: &str,
    ) {
        if !self.switched() {
            return;
        }
        self.write(Call {
            assertion,
            contract: contract.to_owned(),
            verdict: Verdict::Skip,
            aborting: mode == Mode::Fatal,
            location: Some(self.location),
            detail,
            error: None,
            reason: Some(reason.to_owned()),
        });
        self.seat.skip(reason);
    }

    /// Reports that a property failed with `failure`, whose call record states
    /// `detail`, the detail of the run as plain JSON. Under [`Mode::Fatal`] it
    /// may not return.
    #[track_caller]
    pub(crate) fn fail_run(&self, mode: Mode, failure: &Failure, detail: Json) {
        self.seat.helper();
        if !self.switched() {
            return;
        }
        self.write(Call {
            assertion: failure.assertion().unwrap_or_default(),
            contract: failure.contract().to_owned(),
            verdict: Verdict::Fail,
            aborting: mode == Mode::Fatal,
            location: Some(self.location),
            detail: Some(detail),
            error: None,
            reason: None,
        });
        self.seat.report(failure, mode == Mode::Fatal);
    }

    /// Reports that the call passed.
    #[track_caller]
    pub fn pass(&self, mode: Mode, assertion: &'static str, contract: &str) {
        if !self.switched() {
            return;
        }
        self.write(Call {
            assertion,
            contract: contract.to_owned(),
            verdict: Verdict::Pass,
            aborting: mode == Mode::Fatal,
            location: Some(self.location),
            detail: None,
            error: None,
            reason: None,
        });
    }

    /// Reports that the call failed with `detail`, which contains exactly the
    /// fields that the assertion declares. Under [`Mode::Fatal`] it may not
    /// return.
    #[track_caller]
    pub fn fail(
        &self,
        mode: Mode,
        assertion: &'static str,
        contract: &str,
        detail: Vec<(&'static str, Value)>,
    ) {
        self.seat.helper();
        if !self.switched() {
            return;
        }
        let stated = Json::Object(
            detail
                .iter()
                .map(|(name, value)| ((*name).to_owned(), value.to_literal()))
                .collect(),
        );
        self.write(Call {
            assertion,
            contract: contract.to_owned(),
            verdict: Verdict::Fail,
            aborting: mode == Mode::Fatal,
            location: Some(self.location),
            detail: Some(stated),
            error: None,
            reason: None,
        });
        let failure = Failure::new(assertion, contract, detail, Some(self.location));
        self.seat.report(&failure, mode == Mode::Fatal);
    }

    /// Reports that the call ended without a verdict, because `error`
    /// refused an argument or the environment. It ends the call through the
    /// seat's `report_fault` on either surface, and may not return.
    #[track_caller]
    pub fn fault(&self, mode: Mode, assertion: &'static str, contract: &str, error: impl Display) {
        self.fault_with(mode, assertion, contract, Arc::new(Fault::new(error)));
    }

    /// Reports that `fault` ended the call without a verdict, as
    /// [`Run::fault`] does, and hands the seat the fault itself.
    #[track_caller]
    pub(crate) fn fault_with(
        &self,
        mode: Mode,
        assertion: &'static str,
        contract: &str,
        fault: Arc<dyn Error + Send + Sync>,
    ) {
        self.seat.helper();
        if !self.switched() {
            return;
        }
        self.write(Call {
            assertion,
            contract: contract.to_owned(),
            verdict: Verdict::Error,
            aborting: mode == Mode::Fatal,
            location: Some(self.location),
            detail: None,
            error: Some(fault.to_string()),
            reason: None,
        });
        self.seat.report_fault(fault, true);
    }

    /// Returns `true` when the switch of recording states a value. When it
    /// does not, the call ends with the switch's fault.
    #[track_caller]
    fn switched(&self) -> bool {
        if let Switch::Fault(text) = record::switch() {
            end(self.seat, text);
            return false;
        }
        true
    }

    /// Writes the call's record into its slot, for a call that runs a body,
    /// and into the calls of the seat for any other call.
    fn write(&self, call: Call) {
        if let Some(slot) = &self.slot {
            slot.write(call);
            return;
        }
        if let Some(calls) = self.seat.calls() {
            calls.add(call);
        }
    }
}

/// Returns `true` unless the switch states a fault. The fault is reported at
/// the verdict, so a call that runs a body starts its slot only when the
/// switch reads.
fn switched_on() -> bool {
    !matches!(record::switch(), Switch::Fault(_))
}
