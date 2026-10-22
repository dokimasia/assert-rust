// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `soft`, the surface whose failures let the test continue.

use crate::helpers::{names_a_spec, records};
use dokimi_assert::{Recorder, Seat, Stop};
use dokimi_assert_tokio::soft;
use std::time::Duration;

/// Asserts that `seat` reports one failure of `assertion`, through `record`,
/// with a call record that does not abort and a call site in this file.
fn lets_the_test_continue(seat: &Recorder, assertion: &str) {
    let failures = seat.failures();
    assert_eq!(failures.len(), 1, "{failures:?}");
    assert_eq!(failures[0].assertion(), Some(assertion));
    assert_eq!(seat.messages(), [failures[0].to_string()]);
    assert_eq!(records(seat)[0]["aborting"], false);
    assert!(names_a_spec(seat));
    assert!(seat.helper_calls() > 0);
}

mod honours_cancellation {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn records_a_subject_that_ignores_the_handle() {
        let seat = Recorder::new();
        soft::honours_cancellation(&seat, async |_| Ok::<(), Stop>(()), "c").await;
        lets_the_test_continue(&seat, "honours-cancellation");
    }
}

mod honours_deadline {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn records_a_subject_that_ignores_the_handle() {
        let seat = Recorder::new();
        soft::honours_deadline(&seat, async |_| Ok::<(), Stop>(()), "c").await;
        lets_the_test_continue(&seat, "honours-deadline");
    }
}

mod completes_within {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn records_a_body_that_overruns() {
        let seat = Recorder::new();
        let slow = tokio::time::sleep(Duration::from_secs(60));
        soft::completes_within(&seat, Duration::from_millis(10), slow, "c").await;
        lets_the_test_continue(&seat, "completes-within");
    }
}

mod eventually {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn records_a_body_that_never_passes() {
        let seat = Recorder::new();
        let never = async |attempt: &dyn Seat| attempt.record("not yet");
        soft::eventually(
            &seat,
            Duration::from_millis(5),
            Duration::from_millis(1),
            never,
            "c",
        )
        .await;
        lets_the_test_continue(&seat, "eventually");
    }
}

mod eventually_true {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn records_a_predicate_that_stays_false() {
        let seat = Recorder::new();
        soft::eventually_true(&seat, Duration::from_millis(5), async || false, "c").await;
        lets_the_test_continue(&seat, "eventually-true");
    }
}

mod no_task_leaks {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn records_a_task_that_outlives_the_body() {
        let seat = Recorder::new();
        let leaking = async || drop(tokio::spawn(std::future::pending::<()>()));
        soft::no_task_leaks(&seat, leaking, "c").await;
        lets_the_test_continue(&seat, "no-task-leaks");
    }
}
