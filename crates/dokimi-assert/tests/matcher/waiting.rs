//! The spec of `matcher/waiting`: assertions that retry until a condition
//! holds.

use crate::helpers::records;
use dokimi_assert::{Clock, Controlled, Recorder, Seat, System, Value, check, soft};
use std::cell::Cell;
use std::sync::Arc;
use std::time::Duration;

/// Returns a recorder on a controlled clock, with the clock.
fn controlled() -> (Recorder, Arc<Controlled>) {
    let clock = Arc::new(Controlled::new());
    (
        Recorder::new().with_clock(Arc::clone(&clock) as Arc<dyn Clock>),
        clock,
    )
}

mod eventually {
    use super::*;

    #[test]
    fn passes_a_body_that_passes_at_once() {
        let (seat, clock) = controlled();
        check::eventually(
            &seat,
            Duration::from_secs(1),
            Duration::from_millis(10),
            |_| {},
            "c",
        );
        assert!(!seat.failed());
        assert_eq!(clock.now(), Duration::ZERO);
    }

    #[test]
    fn advances_a_controlled_clock_by_the_interval_between_attempts() {
        let (seat, clock) = controlled();
        let attempts = Cell::new(0);
        let settles = |attempt: &dyn Seat| {
            attempts.set(attempts.get() + 1);
            check::is_true(attempt, attempts.get() >= 3, "the subject has settled");
        };
        check::eventually(
            &seat,
            Duration::from_secs(1),
            Duration::from_millis(10),
            settles,
            "c",
        );
        assert!(!seat.failed());
        assert_eq!(clock.now(), Duration::from_millis(20));
    }

    #[test]
    fn sleeps_on_the_platform_clock_between_attempts() {
        let seat = Recorder::new().with_clock(Arc::new(System::new()));
        let never = |attempt: &dyn Seat| check::is_true(attempt, false, "the subject has settled");
        check::eventually(
            &seat,
            Duration::from_millis(200),
            Duration::from_millis(10),
            never,
            "c",
        );
        let Some(Value::Int(attempts)) = seat.failures()[0].field("attempts").cloned() else {
            panic!("the failure states its attempts");
        };
        assert!(
            (2..=21).contains(&attempts),
            "{attempts} attempts in 200ms at an interval of 10ms"
        );
    }

    #[test]
    fn states_the_attempts_with_the_first_failure_of_the_last() {
        let (seat, _) = controlled();
        let never = |attempt: &dyn Seat| {
            check::is_true(attempt, false, "the subject has settled");
            check::is_true(attempt, false, "never reached");
        };
        check::eventually(
            &seat,
            Duration::from_millis(100),
            Duration::from_millis(10),
            never,
            "c",
        );
        let failure = &seat.failures()[0];
        assert_eq!(failure.assertion(), Some("eventually"));
        assert_eq!(
            failure.detail(),
            [
                ("attempts", Value::Int(11)),
                ("last", Value::from("the subject has settled"))
            ]
        );
    }

    #[test]
    fn runs_the_body_once_for_a_timeout_of_zero() {
        let (seat, _) = controlled();
        let runs = Cell::new(0);
        let unsettled = |attempt: &dyn Seat| {
            runs.set(runs.get() + 1);
            attempt.record("not yet");
        };
        soft::eventually(
            &seat,
            Duration::ZERO,
            Duration::from_millis(10),
            unsettled,
            "c",
        );
        assert_eq!(runs.get(), 1);
        assert_eq!(seat.messages(), ["c: attempts 1, last \"not yet\""]);
    }

    #[test]
    fn fails_an_attempt_that_passes_after_the_deadline() {
        let (seat, clock) = controlled();
        let late = |_: &dyn Seat| clock.advance(Duration::from_millis(11));
        check::eventually(
            &seat,
            Duration::from_millis(10),
            Duration::from_millis(1),
            late,
            "c",
        );
        assert_eq!(seat.failures()[0].field("attempts"), Some(&Value::Int(1)));
    }

    #[test]
    fn records_the_calls_of_each_attempt_under_the_call() {
        let (seat, _) = controlled();
        let attempts = Cell::new(0);
        let second = |attempt: &dyn Seat| {
            attempts.set(attempts.get() + 1);
            check::is_true(attempt, attempts.get() == 2, "the subject has settled");
        };
        check::eventually(
            &seat,
            Duration::from_secs(1),
            Duration::from_millis(10),
            second,
            "the store settles",
        );
        let summary: Vec<String> = records(&seat)
            .iter()
            .map(|record| {
                let members = ["seq", "assertion", "verdict", "parent", "run"];
                members.map(|name| record[name].to_string()).join(" ")
            })
            .collect();
        assert_eq!(
            summary,
            [
                r#"1 "eventually" "pass" null null"#,
                r#"2 "true" "fail" 1 1"#,
                r#"3 "true" "pass" 1 2"#,
            ]
        );
    }

    #[test]
    fn waits_at_least_a_millisecond_between_attempts() {
        let (seat, clock) = controlled();
        let attempts = Cell::new(0);
        let third = |attempt: &dyn Seat| {
            attempts.set(attempts.get() + 1);
            check::is_true(attempt, attempts.get() == 3, "c");
        };
        check::eventually(&seat, Duration::from_secs(1), Duration::ZERO, third, "c");
        assert_eq!(clock.now(), Duration::from_millis(2));
    }
}

mod eventually_true {
    use super::*;

    #[test]
    fn passes_a_predicate_that_becomes_true() {
        let (seat, clock) = controlled();
        let calls = Cell::new(0);
        let fourth = || {
            calls.set(calls.get() + 1);
            calls.get() == 4
        };
        check::eventually_true(&seat, Duration::from_secs(1), fourth, "c");
        assert!(!seat.failed());
        assert_eq!(clock.now(), Duration::from_millis(1 + 2 + 4));
    }

    #[test]
    fn caps_the_backoff_at_a_quarter_of_the_timeout() {
        let (seat, clock) = controlled();
        let calls = Cell::new(0);
        let sixth = || {
            calls.set(calls.get() + 1);
            calls.get() == 6
        };
        check::eventually_true(&seat, Duration::from_millis(16), sixth, "c");
        assert!(!seat.failed());
        assert_eq!(clock.now(), Duration::from_millis(1 + 2 + 4 + 4 + 4));
    }

    #[test]
    fn states_the_attempts_of_a_predicate_that_stays_false() {
        let (seat, _) = controlled();
        soft::eventually_true(&seat, Duration::from_millis(16), || false, "c");
        let failure = &seat.failures()[0];
        assert_eq!(failure.assertion(), Some("eventually-true"));
        assert_eq!(failure.detail(), [("attempts", Value::Int(7))]);
    }
}
