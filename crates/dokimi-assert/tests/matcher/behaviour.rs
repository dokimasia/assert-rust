// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `matcher/behaviour`: how a subject behaves, rather than what it
//! returns.

use crate::helpers::{failure_of, opaque, panic_of, passes};
use dokimi_assert::{Cancel, Clock, Controlled, Recorder, Stop, Value, check, soft};
use std::cell::Cell;
use std::error::Error;
use std::fmt;
use std::sync::{Arc, mpsc};
use std::time::Duration;

/// A failure of a subject's own.
#[derive(Debug)]
struct Own;

impl fmt::Display for Own {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a failure of the subject's own")
    }
}

impl Error for Own {}

/// An error of a subject that stopped, with the reason as its source.
#[derive(Debug)]
struct Interrupted(Stop);

impl fmt::Display for Interrupted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "the fetch stopped: {}", self.0)
    }
}

impl Error for Interrupted {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.0)
    }
}

/// Returns the reason of the handle as the subject's error.
fn reads_handle(cancel: Option<&Cancel>) -> Result<(), Stop> {
    cancel.and_then(Cancel::reason).map_or(Ok(()), Err)
}

/// Returns a recorder on a controlled clock, with the clock.
fn controlled() -> (Recorder, Arc<Controlled>) {
    let clock = Arc::new(Controlled::new());
    (
        Recorder::new().with_clock(Arc::clone(&clock) as Arc<dyn Clock>),
        clock,
    )
}

mod honours_cancellation {
    use super::*;

    #[test]
    fn passes_a_subject_that_returns_the_reason() {
        passes(|t| check::honours_cancellation(t, reads_handle, "c"));
    }

    #[test]
    fn passes_a_subject_whose_error_has_the_reason_as_a_source() {
        passes(|t| {
            check::honours_cancellation(t, |cancel| reads_handle(cancel).map_err(Interrupted), "c");
        });
    }

    #[test]
    fn hands_the_subject_a_cancelled_handle() {
        let reason = Cell::new(None);
        let record = |cancel: Option<&Cancel>| {
            reason.set(cancel.and_then(Cancel::reason));
            Ok::<(), Own>(())
        };
        let _ = failure_of(|t| check::honours_cancellation(t, record, "c"));
        assert_eq!(reason.get(), Some(Stop::Cancelled));
    }

    #[test]
    fn states_null_for_a_subject_that_returns_ok() {
        let failure = failure_of(|t| check::honours_cancellation(t, |_| Ok::<(), Own>(()), "c"));
        assert_eq!(failure.assertion(), Some("honours-cancellation"));
        assert_eq!(failure.detail(), [("got", Value::Null)]);
    }

    #[test]
    fn states_an_error_of_the_subjects_own() {
        let failure = failure_of(|t| soft::honours_cancellation(t, |_| Err::<(), _>(Own), "c"));
        assert_eq!(failure.got(), Some(&opaque("Own")));
    }

    #[test]
    fn states_the_message_of_a_subject_that_panics() {
        let panicking = |_: Option<&Cancel>| -> Result<(), Own> { panic!("unwrapped a None") };
        let failure = failure_of(|t| check::honours_cancellation(t, panicking, "c"));
        assert_eq!(failure.got(), Some(&Value::from("unwrapped a None")));
    }
}

mod honours_deadline {
    use super::*;

    #[test]
    fn passes_a_subject_that_returns_the_reason() {
        passes(|t| check::honours_deadline(t, reads_handle, "c"));
    }

    #[test]
    fn fails_a_subject_that_returns_another_reason() {
        let failure =
            failure_of(|t| check::honours_deadline(t, |_| Err::<(), _>(Stop::Cancelled), "c"));
        assert_eq!(failure.assertion(), Some("honours-deadline"));
        assert_eq!(failure.got(), Some(&opaque("Cancelled")));
    }
}

mod none_handle_safe {
    use super::*;

    #[test]
    fn passes_a_subject_that_returns() {
        passes(|t| check::none_handle_safe(t, reads_handle, "c"));
    }

    #[test]
    fn passes_an_error_of_the_subjects_own() {
        passes(|t| check::none_handle_safe(t, |_| Err::<(), _>(Own), "c"));
    }

    #[test]
    fn states_the_message_of_a_subject_that_panics() {
        let dereferences = |cancel: Option<&Cancel>| {
            let _ = cancel.expect("the subject expects a handle");
            Ok::<(), Own>(())
        };
        let failure = failure_of(|t| check::none_handle_safe(t, dereferences, "c"));
        assert_eq!(failure.assertion(), Some("nil-context-safe"));
        assert_eq!(
            failure.detail(),
            [("got", Value::from("the subject expects a handle"))]
        );
    }
}

mod completes_within {
    use super::*;

    #[test]
    fn passes_a_body_that_returns_in_time() {
        passes(|t| check::completes_within(t, Duration::from_secs(5), |_| Ok::<(), Own>(()), "c"));
    }

    #[test]
    fn passes_a_body_that_returns_its_own_error_in_time() {
        passes(|t| check::completes_within(t, Duration::from_secs(5), |_| Err::<(), _>(Own), "c"));
    }

    #[test]
    fn states_the_time_on_the_clock_of_the_seat_in_nanoseconds() {
        let (seat, clock) = controlled();
        let advanced = Arc::clone(&clock);
        let slow = move |_: Option<&Cancel>| {
            advanced.advance(Duration::from_millis(6_000_400));
            Ok::<(), Own>(())
        };
        check::completes_within(&seat, Duration::from_secs(5), slow, "c");
        let failure = &seat.failures()[0];
        assert_eq!(failure.assertion(), Some("completes-within"));
        assert_eq!(
            failure.detail(),
            [
                ("want", Value::Int(5_000_000_000)),
                ("got", Value::Int(6_000_400_000_000))
            ]
        );
    }

    #[test]
    fn rounds_a_half_millisecond_up() {
        let (seat, clock) = controlled();
        let advanced = Arc::clone(&clock);
        let slow = move |_: Option<&Cancel>| {
            advanced.advance(Duration::from_micros(1_000_500));
            Ok::<(), Own>(())
        };
        check::completes_within(&seat, Duration::from_secs(1), slow, "c");
        assert_eq!(seat.failures()[0].got(), Some(&Value::Int(1_001_000_000)));
    }

    #[test]
    fn expires_the_handle_of_a_body_that_overruns() {
        let (release, released) = mpsc::channel::<()>();
        let (handles, handed) = mpsc::channel::<Cancel>();
        let overrunning = move |cancel: Option<&Cancel>| {
            let _ = handles.send(cancel.expect("the body receives a handle").clone());
            let _ = released.recv();
            Ok::<(), Own>(())
        };
        let failure =
            failure_of(|t| soft::completes_within(t, Duration::from_millis(10), overrunning, "c"));
        drop(release);
        let handle = handed.recv().expect("the body hands over its handle");
        assert_eq!(handle.reason(), Some(Stop::DeadlineExceeded));
        assert_eq!(failure.want(), Some(&Value::Int(10_000_000)));
        assert!(
            matches!(failure.got(), Some(Value::Int(got)) if *got >= 10_000_000 && got % 1_000_000 == 0),
            "{failure:?}"
        );
    }

    #[test]
    fn panics_again_with_the_panic_of_the_body() {
        let seat = Recorder::new();
        let broken = |_: Option<&Cancel>| -> Result<(), Own> { panic!("the body broke") };
        let raised =
            panic_of(|| check::completes_within(&seat, Duration::from_secs(5), broken, "c"));
        assert_eq!(raised.as_deref(), Some("the body broke"));
        assert!(!seat.failed());
    }
}

mod is_pure {
    use super::*;

    #[test]
    fn passes_a_body_that_leaves_the_reading_alone() {
        let count = Cell::new(1);
        let read = || {
            let _ = count.get();
        };
        passes(|t| check::is_pure(t, || count.get(), read, "c"));
    }

    #[test]
    fn states_the_readings_before_and_after() {
        let count = Cell::new(1);
        let failure = failure_of(|t| check::is_pure(t, || count.get(), || count.set(2), "c"));
        assert_eq!(failure.assertion(), Some("pure"));
        assert_eq!(
            failure.detail(),
            [("want", opaque("1")), ("got", opaque("2"))]
        );
    }
}
