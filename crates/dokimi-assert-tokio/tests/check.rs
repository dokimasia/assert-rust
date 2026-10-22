//! The spec of `check`, the surface whose failures end the test.

use crate::helpers::{names_a_spec, records};
use dokimi_assert::{Recorder, Seat, Stop};
use dokimi_assert_tokio::check;
use std::time::Duration;

/// Asserts that `seat` reports one failure of `assertion`, through `fail`,
/// with an aborting call record and a call site in this file.
fn ends_the_test(seat: &Recorder, assertion: &str) {
    let failures = seat.failures();
    assert_eq!(failures.len(), 1, "{failures:?}");
    assert_eq!(failures[0].assertion(), Some(assertion));
    assert_eq!(seat.message(), failures[0].to_string());
    assert_eq!(seat.messages(), [] as [String; 0]);
    assert_eq!(records(seat)[0]["aborting"], true);
    assert!(names_a_spec(seat));
    assert!(seat.helper_calls() > 0);
}

mod honours_cancellation {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn ends_the_test_at_a_subject_that_ignores_the_handle() {
        let seat = Recorder::new();
        check::honours_cancellation(&seat, async |_| Ok::<(), Stop>(()), "c").await;
        ends_the_test(&seat, "honours-cancellation");
    }
}

mod honours_deadline {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn ends_the_test_at_a_subject_that_ignores_the_handle() {
        let seat = Recorder::new();
        check::honours_deadline(&seat, async |_| Ok::<(), Stop>(()), "c").await;
        ends_the_test(&seat, "honours-deadline");
    }
}

mod completes_within {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn ends_the_test_at_a_body_that_overruns() {
        let seat = Recorder::new();
        let slow = tokio::time::sleep(Duration::from_secs(60));
        check::completes_within(&seat, Duration::from_millis(10), slow, "c").await;
        ends_the_test(&seat, "completes-within");
    }
}

mod eventually {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn ends_the_test_at_a_body_that_never_passes() {
        let seat = Recorder::new();
        let never = async |attempt: &dyn Seat| attempt.record("not yet");
        check::eventually(
            &seat,
            Duration::from_millis(5),
            Duration::from_millis(1),
            never,
            "c",
        )
        .await;
        ends_the_test(&seat, "eventually");
    }
}

mod eventually_true {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn ends_the_test_at_a_predicate_that_stays_false() {
        let seat = Recorder::new();
        check::eventually_true(&seat, Duration::from_millis(5), async || false, "c").await;
        ends_the_test(&seat, "eventually-true");
    }
}

mod no_task_leaks {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn ends_the_test_at_a_task_that_outlives_the_body() {
        let seat = Recorder::new();
        let leaking = async || drop(tokio::spawn(std::future::pending::<()>()));
        check::no_task_leaks(&seat, leaking, "c").await;
        ends_the_test(&seat, "no-task-leaks");
    }
}
