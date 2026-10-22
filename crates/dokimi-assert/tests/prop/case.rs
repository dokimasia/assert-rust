//! The spec of `prop::case`, one call of a property's body and the seat of
//! its assertions.

use crate::helpers::records;
use dokimi_assert::prop::engine::harness;
use dokimi_assert::prop::{self, Case};
use dokimi_assert::{Cancel, Clock, Controlled, Recorder, Seat, Stop, check, soft};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Returns the recorder of a run of `body` with the seed 1.
fn run(body: impl Fn(&Case<'_>) + Send + Sync) -> Recorder {
    let seat = Recorder::new();
    prop::for_all(
        &seat,
        "the case holds",
        prop::hermetic().seed(1).cases(20).store(""),
        body,
    );
    seat
}

/// Returns the detail of the first call record of `seat`.
fn detail(seat: &Recorder) -> serde_json::Value {
    records(seat)[0]["detail"].clone()
}

mod case {
    use super::*;

    #[test]
    fn states_its_record_in_its_debug_text() {
        let seat = run(|c| {
            let _ = c.draw(&prop::boolean(), "flag");
            assert!(format!("{c:?}").starts_with("Case { record: Record { choices: ["));
        });
        assert!(!seat.failed(), "{}", seat.message());
    }
}

mod assume {
    use super::*;

    #[test]
    fn rejects_a_case_whose_condition_is_false() {
        let seat = run(|c| {
            let n = c.draw(&prop::integer(0..=9_u8), "n");
            c.assume(n < 100);
            c.assume(false);
        });
        assert_eq!(detail(&seat)["outcome"], "rejected");
    }
}

mod classify {
    use super::*;

    #[test]
    fn counts_a_label_that_a_requirement_reads() {
        let seat = Recorder::new();
        prop::for_all(
            &seat,
            "the labels count",
            prop::hermetic().seed(1).require("small", 0.99).store(""),
            |c| {
                let n = c.draw(&prop::integer(0..=9_u8), "n");
                if n < 5 {
                    c.classify("small");
                    c.classify("small");
                }
            },
        );
        assert_eq!(detail(&seat)["outcome"], "coverage-unmet");
        assert_eq!(detail(&seat)["coverage"]["label"], "small");
    }
}

mod note {
    use super::*;

    #[test]
    fn states_its_message_in_the_report_of_a_failing_case() {
        let seat = run(|c| {
            let n = c.draw(&prop::integer(0..=9_u8), "n");
            c.note(&format!("n is {n}"));
            check::is_true(c, n < 3, "n stays below 3");
        });
        assert!(
            seat.message().ends_with("note: n is 3"),
            "{}",
            seat.message()
        );
    }
}

mod observe {
    use super::*;

    #[test]
    fn makes_a_run_flaky_when_a_replay_observes_another_fingerprint() {
        let calls = AtomicUsize::new(0);
        let seat = run(|c| {
            let n = c.draw(&prop::integer(0..=9_u8), "n");
            c.observe(calls.fetch_add(1, Ordering::SeqCst) as u64);
            check::is_true(c, n < 3, "n stays below 3");
        });
        assert_eq!(detail(&seat)["outcome"], "flaky");
        assert_eq!(detail(&seat)["divergence"]["what"], "fingerprint");
    }
}

mod target {
    use super::*;

    #[test]
    fn keeps_the_higher_of_two_scores_without_a_change_to_the_run() {
        let seat = run(|c| {
            let n = c.draw(&prop::integer(0..=9_u8), "n");
            c.target("n", f64::from(n));
            c.target("n", 0.0);
        });
        assert!(!seat.failed(), "{}", seat.message());
    }
}

mod cleanup {
    use super::*;

    #[test]
    fn runs_the_last_registered_cleanup_first_after_the_body() {
        let order = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&order);
        let seat = Recorder::new();
        prop::for_all(
            &seat,
            "the cleanups run",
            prop::hermetic().seed(1).cases(1).store(""),
            move |c| {
                let _ = c.draw(&prop::boolean(), "flag");
                for name in ["first", "second"] {
                    let order = Arc::clone(&seen);
                    c.cleanup(move || order.lock().expect("no cleanup panicked").push(name));
                }
            },
        );
        let order = order.lock().expect("no cleanup panicked").clone();
        assert_eq!(&order[..2], ["second", "first"]);
    }

    #[test]
    fn fails_the_case_with_a_failure_in_a_cleanup() {
        let seat = run(|c| {
            let _ = c.draw(&prop::boolean(), "flag");
            c.cleanup(|| panic!("the cleanup fails"));
        });
        let case = seat.failures()[0]
            .case_failure()
            .cloned()
            .expect("the run found a counterexample");
        assert_eq!(case.contract(), "the cleanup fails");
    }
}

mod rand {
    use super::*;

    #[test]
    fn takes_each_value_from_a_choice_of_the_case() {
        let seat = run(|c| {
            let mut rand = c.rand();
            assert_eq!(format!("{rand:?}"), "Rand { .. }");
            let wide = rand.next_u64();
            let narrow = rand.next_u32();
            let mut bytes = [0_u8; 12];
            rand.fill_bytes(&mut bytes);
            check::is_true(
                c,
                wide < u64::MAX || narrow < u32::MAX || bytes.len() == 12,
                "the source returns values",
            );
        });
        assert!(!seat.failed(), "{}", seat.message());
    }

    #[test]
    fn shrinks_a_value_that_it_returned_with_the_case() {
        let seat = run(|c| {
            let value = c.rand().next_u64();
            check::is_true(c, value < 1000, "the value stays below 1000");
        });
        assert!(seat.message().contains("valid"), "{}", seat.message());
        assert_eq!(detail(&seat)["outcome"], "counterexample");
    }

    #[cfg(feature = "rand_core")]
    #[test]
    fn implements_the_fallible_source_of_rand_core() {
        use rand_core::TryRng;
        let seat = run(|c| {
            let mut rand = c.rand();
            let _ = rand.try_next_u64();
            let _ = rand.try_next_u32();
            let mut bytes = [0_u8; 3];
            let filled = rand.try_fill_bytes(&mut bytes);
            check::is_true(c, filled.is_ok(), "the source fills the bytes");
        });
        assert!(!seat.failed(), "{}", seat.message());
    }
}

mod history {
    use super::*;

    #[test]
    fn starts_each_case_with_an_empty_history() {
        let seat = run(|c| {
            let _ = c.draw(&prop::boolean(), "flag");
            check::length(c, &c.history().events(), 0, "the history is empty");
            c.history().invoke(0, "read", [], []).ok(1);
        });
        assert!(!seat.failed(), "{}", seat.message());
    }
}

mod seat {
    use super::*;

    #[test]
    fn ends_the_case_with_a_message_that_fail_reports() {
        let seat = run(|c| {
            let _ = c.draw(&prop::boolean(), "flag");
            c.fail("the case fails");
            c.fail("the case never gets here");
        });
        let case = seat.failures()[0]
            .case_failure()
            .cloned()
            .expect("the run found a counterexample");
        assert_eq!(case.contract(), "the case fails");
        assert_eq!(case.assertion(), None);
    }

    #[test]
    fn continues_the_case_after_a_message_that_record_reports() {
        let after = AtomicUsize::new(0);
        let seat = run(|c| {
            let _ = c.draw(&prop::boolean(), "flag");
            c.record("the case fails");
            after.fetch_add(1, Ordering::SeqCst);
            soft::is_true(c, false, "the flag is set");
        });
        assert!(after.load(Ordering::SeqCst) > 0);
        let case = seat.failures()[0]
            .case_failure()
            .cloned()
            .expect("the run found a counterexample");
        assert_eq!(case.contract(), "the case fails");
    }

    #[test]
    fn ends_every_call_after_the_end_of_the_case() {
        let ends =
            |call: &dyn Fn()| std::panic::catch_unwind(std::panic::AssertUnwindSafe(call)).is_err();
        let seat = run(|c| {
            let _ = c.draw(&prop::boolean(), "flag");
            assert!(ends(&|| c.assume(false)));
            assert!(ends(&|| c.assume(true)));
            assert!(ends(&|| c.fail("a failure after the end")));
            assert!(ends(&|| check::is_true(c, false, "a check after the end")));
            assert!(ends(&|| _ = c.rand().next_u64()));
            c.record("a record after the end");
        });
        assert_eq!(detail(&seat)["outcome"], "rejected");
    }

    #[test]
    fn reads_the_platform_clock_without_a_property() {
        let timed = prop::composite(|c| {
            check::completes_within(
                c,
                Duration::from_secs(60),
                |_| Ok::<(), String>(()),
                "the body ends in time",
            );
            1_u8
        });
        assert_eq!(harness::decode(&timed, Vec::new()).1, Some(1));
    }

    #[test]
    fn reads_the_clock_of_the_seat_of_the_property() {
        let clock = Arc::new(Controlled::new());
        clock.advance(Duration::from_secs(5));
        let seat = Recorder::new().with_clock(Arc::clone(&clock) as Arc<dyn Clock>);
        prop::for_all(
            &seat,
            "the case reads the clock",
            prop::hermetic().seed(1).cases(3).store(""),
            |c| {
                let _ = c.draw(&prop::boolean(), "flag");
                check::equal(
                    c,
                    c.clock().now(),
                    Duration::from_secs(5),
                    "the case reads the seat's clock",
                );
            },
        );
        assert!(!seat.failed(), "{}", seat.message());
    }

    #[test]
    fn stops_its_cancellation_handle_when_it_ends() {
        let handles: Arc<Mutex<Vec<Cancel>>> = Arc::new(Mutex::new(Vec::new()));
        let kept = Arc::clone(&handles);
        let seat = Recorder::new();
        prop::for_all(
            &seat,
            "the case ends",
            prop::hermetic().seed(1).cases(2).store(""),
            move |c| {
                let _ = c.draw(&prop::boolean(), "flag");
                check::is_true(
                    c,
                    c.cancellation().reason().is_none(),
                    "the handle runs during the body",
                );
                kept.lock()
                    .expect("no body panicked")
                    .push(c.cancellation().clone());
            },
        );
        let handles = handles.lock().expect("no body panicked");
        assert!(
            handles
                .iter()
                .all(|handle| handle.reason() == Some(Stop::Cancelled))
        );
    }
}
