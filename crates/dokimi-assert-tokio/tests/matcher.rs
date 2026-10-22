// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `matcher`: the comparisons that both surfaces share.

use crate::helpers::records;
use dokimi_assert::{Cancel, Recorder, Seat, Stop, Value};
use dokimi_assert_tokio::{check, soft};
use std::any::Any;
use std::cell::{Cell, RefCell};
use std::error::Error;
use std::fmt;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::pin::pin;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};
use std::time::Duration;
use tokio::time::Instant;

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

/// Waits until the handle stops, and returns its reason as the subject's
/// error.
async fn reads_handle(cancel: Option<&Cancel>) -> Result<(), Stop> {
    let cancel = cancel.expect("the subject receives a handle");
    Err(cancel.stopped().await)
}

/// Runs `future` on a runtime of its own whose clock starts paused, and
/// returns the payload of its panic.
fn panic_of(future: impl Future<Output = ()>) -> Box<dyn Any + Send> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .start_paused(true)
        .build()
        .expect("the test builds a runtime");
    catch_unwind(AssertUnwindSafe(|| runtime.block_on(future))).expect_err("the future panics")
}

/// Runs `future` to its end on the calling thread, outside any tokio runtime.
fn block_on<F: Future>(future: F) -> F::Output {
    /// Wakes the thread that waits for the future.
    struct Unpark(std::thread::Thread);

    impl Wake for Unpark {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }

    let waker = Waker::from(Arc::new(Unpark(std::thread::current())));
    let mut cx = Context::from_waker(&waker);
    let mut future = pin!(future);
    loop {
        if let Poll::Ready(output) = future.as_mut().poll(&mut cx) {
            return output;
        }
        std::thread::park();
    }
}

mod honours_cancellation {
    use super::*;

    #[tokio::test]
    async fn passes_a_subject_that_returns_the_reason() {
        let seat = Recorder::new();
        check::honours_cancellation(&seat, reads_handle, "c").await;
        assert!(!seat.failed(), "{}", seat.message());
    }

    #[tokio::test]
    async fn passes_a_subject_whose_error_has_the_reason_as_a_source() {
        let seat = Recorder::new();
        let interrupted =
            async |cancel: Option<&Cancel>| reads_handle(cancel).await.map_err(Interrupted);
        check::honours_cancellation(&seat, interrupted, "c").await;
        assert!(!seat.failed(), "{}", seat.message());
    }

    #[test]
    fn passes_on_an_executor_other_than_tokio() {
        let seat = Recorder::new();
        block_on(check::honours_cancellation(&seat, reads_handle, "c"));
        assert!(!seat.failed(), "{}", seat.message());
    }

    #[tokio::test]
    async fn hands_the_subject_a_cancelled_handle() {
        let seat = Recorder::new();
        let reason = Cell::new(None);
        let keeps_the_reason = async |cancel: Option<&Cancel>| {
            reason.set(cancel.and_then(Cancel::reason));
            Ok::<(), Own>(())
        };
        check::honours_cancellation(&seat, keeps_the_reason, "c").await;
        assert_eq!(reason.get(), Some(Stop::Cancelled));
    }

    #[tokio::test]
    async fn states_null_for_a_subject_that_returns_ok() {
        let seat = Recorder::new();
        check::honours_cancellation(&seat, async |_| Ok::<(), Own>(()), "c").await;
        let failure = &seat.failures()[0];
        assert_eq!(failure.assertion(), Some("honours-cancellation"));
        assert_eq!(failure.detail(), [("got", Value::Null)]);
    }

    #[tokio::test]
    async fn states_an_error_of_the_subjects_own() {
        let seat = Recorder::new();
        soft::honours_cancellation(&seat, async |_| Err::<(), _>(Own), "c").await;
        assert_eq!(
            seat.failures()[0].got(),
            Some(&Value::Opaque("Own".to_owned()))
        );
    }

    #[tokio::test]
    async fn states_the_message_of_a_subject_that_panics() {
        let seat = Recorder::new();
        let panicking =
            async |_: Option<&Cancel>| -> Result<(), Own> { panic!("unwrapped a None") };
        check::honours_cancellation(&seat, panicking, "c").await;
        assert_eq!(
            seat.failures()[0].got(),
            Some(&Value::from("unwrapped a None"))
        );
    }
}

mod honours_deadline {
    use super::*;

    #[tokio::test]
    async fn passes_a_subject_that_returns_the_reason() {
        let seat = Recorder::new();
        check::honours_deadline(&seat, reads_handle, "c").await;
        assert!(!seat.failed(), "{}", seat.message());
    }

    #[tokio::test]
    async fn fails_a_subject_that_returns_another_reason() {
        let seat = Recorder::new();
        check::honours_deadline(&seat, async |_| Err::<(), _>(Stop::Cancelled), "c").await;
        let failure = &seat.failures()[0];
        assert_eq!(failure.assertion(), Some("honours-deadline"));
        assert_eq!(failure.got(), Some(&Value::Opaque("Cancelled".to_owned())));
    }
}

mod completes_within {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn passes_a_body_that_completes_in_time() {
        let seat = Recorder::new();
        let quick = tokio::time::sleep(Duration::from_millis(5));
        check::completes_within(&seat, Duration::from_millis(10), quick, "c").await;
        assert!(!seat.failed(), "{}", seat.message());
    }

    #[tokio::test(start_paused = true)]
    async fn states_the_duration_and_the_time_waited_in_nanoseconds() {
        let seat = Recorder::new();
        let slow = tokio::time::sleep(Duration::from_secs(60));
        check::completes_within(&seat, Duration::from_millis(10), slow, "c").await;
        let failure = &seat.failures()[0];
        assert_eq!(failure.assertion(), Some("completes-within"));
        assert_eq!(
            failure.detail(),
            [
                ("want", Value::Int(10_000_000)),
                ("got", Value::Int(10_000_000))
            ]
        );
    }

    #[tokio::test(start_paused = true)]
    async fn drops_a_body_that_overruns_at_the_duration() {
        /// Sets its flag when it drops.
        struct Guard<'a>(&'a Cell<bool>);

        impl Drop for Guard<'_> {
            fn drop(&mut self) {
                self.0.set(true);
            }
        }

        let seat = Recorder::new();
        let dropped = Cell::new(false);
        let guarded = async {
            let _guard = Guard(&dropped);
            tokio::time::sleep(Duration::from_secs(60)).await;
        };
        let started = Instant::now();
        soft::completes_within(&seat, Duration::from_millis(10), guarded, "c").await;
        assert!(dropped.get());
        assert_eq!(started.elapsed(), Duration::from_millis(10));
    }

    #[test]
    fn panics_again_with_the_panic_of_the_body() {
        let seat = Recorder::new();
        let broken = async { panic!("the body broke") };
        let raised = panic_of(check::completes_within(
            &seat,
            Duration::from_secs(5),
            broken,
            "c",
        ));
        assert_eq!(raised.downcast_ref::<&str>(), Some(&"the body broke"));
        assert!(!seat.failed());
    }
}

mod eventually {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn passes_a_body_that_passes_at_once() {
        let seat = Recorder::new();
        let started = Instant::now();
        check::eventually(
            &seat,
            Duration::from_secs(1),
            Duration::from_millis(10),
            async |_| {},
            "c",
        )
        .await;
        assert!(!seat.failed(), "{}", seat.message());
        assert_eq!(started.elapsed(), Duration::ZERO);
    }

    #[tokio::test(start_paused = true)]
    async fn waits_the_interval_on_the_clock_of_tokio_between_attempts() {
        let seat = Recorder::new();
        let attempts = Cell::new(0);
        let settles = async |attempt: &dyn Seat| {
            attempts.set(attempts.get() + 1);
            dokimi_assert::check::is_true(attempt, attempts.get() >= 3, "the subject has settled");
        };
        let started = Instant::now();
        check::eventually(
            &seat,
            Duration::from_secs(1),
            Duration::from_millis(10),
            settles,
            "c",
        )
        .await;
        assert!(!seat.failed(), "{}", seat.message());
        assert_eq!(started.elapsed(), Duration::from_millis(20));
    }

    #[tokio::test(start_paused = true)]
    async fn states_the_attempts_with_the_first_failure_of_the_last() {
        let seat = Recorder::new();
        let never = async |attempt: &dyn Seat| {
            dokimi_assert::check::is_true(attempt, false, "the subject has settled");
            dokimi_assert::check::is_true(attempt, false, "never reached");
        };
        check::eventually(
            &seat,
            Duration::from_millis(100),
            Duration::from_millis(10),
            never,
            "c",
        )
        .await;
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

    #[tokio::test(start_paused = true)]
    async fn runs_the_body_once_for_a_timeout_of_zero() {
        let seat = Recorder::new();
        let runs = Cell::new(0);
        let unsettled = async |attempt: &dyn Seat| {
            runs.set(runs.get() + 1);
            attempt.record("not yet");
        };
        soft::eventually(
            &seat,
            Duration::ZERO,
            Duration::from_millis(10),
            unsettled,
            "c",
        )
        .await;
        assert_eq!(runs.get(), 1);
        assert_eq!(seat.messages(), ["c: attempts 1, last \"not yet\""]);
    }

    #[tokio::test(start_paused = true)]
    async fn fails_an_attempt_that_passes_after_the_deadline() {
        let seat = Recorder::new();
        let late = async |_: &dyn Seat| tokio::time::sleep(Duration::from_millis(11)).await;
        check::eventually(
            &seat,
            Duration::from_millis(10),
            Duration::from_millis(1),
            late,
            "c",
        )
        .await;
        assert_eq!(seat.failures()[0].field("attempts"), Some(&Value::Int(1)));
    }

    #[tokio::test(start_paused = true)]
    async fn records_the_calls_of_each_attempt_under_the_call() {
        let seat = Recorder::new();
        let attempts = Cell::new(0);
        let second = async |attempt: &dyn Seat| {
            attempts.set(attempts.get() + 1);
            dokimi_assert::check::is_true(attempt, attempts.get() == 2, "the subject has settled");
        };
        check::eventually(
            &seat,
            Duration::from_secs(1),
            Duration::from_millis(10),
            second,
            "the store settles",
        )
        .await;
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

    #[tokio::test(start_paused = true)]
    async fn waits_at_least_a_millisecond_between_attempts() {
        let seat = Recorder::new();
        let attempts = Cell::new(0);
        let third = async |attempt: &dyn Seat| {
            attempts.set(attempts.get() + 1);
            dokimi_assert::check::is_true(attempt, attempts.get() == 3, "c");
        };
        let started = Instant::now();
        check::eventually(&seat, Duration::from_secs(1), Duration::ZERO, third, "c").await;
        assert_eq!(started.elapsed(), Duration::from_millis(2));
    }

    #[tokio::test(start_paused = true)]
    async fn stops_the_handle_of_each_attempt_when_it_ends() {
        let seat = Recorder::new();
        let handles = RefCell::new(Vec::new());
        let keeps_the_handle = async |attempt: &dyn Seat| {
            handles.borrow_mut().push(attempt.cancellation().clone());
            attempt.record("not yet");
        };
        check::eventually(
            &seat,
            Duration::from_millis(2),
            Duration::from_millis(1),
            keeps_the_handle,
            "c",
        )
        .await;
        let handles = handles.into_inner();
        assert_eq!(handles.len(), 3);
        assert!(handles.iter().all(Cancel::is_stopped));
        assert!(!seat.cancellation().is_stopped());
    }

    #[test]
    fn panics_again_with_the_panic_of_the_body() {
        let seat = Recorder::new();
        let broken = async |_: &dyn Seat| panic!("the body broke");
        let raised = panic_of(check::eventually(
            &seat,
            Duration::from_secs(1),
            Duration::from_millis(10),
            broken,
            "c",
        ));
        assert_eq!(raised.downcast_ref::<&str>(), Some(&"the body broke"));
        assert!(!seat.failed());
    }
}

mod eventually_true {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn passes_a_predicate_that_becomes_true() {
        let seat = Recorder::new();
        let calls = Cell::new(0);
        let fourth = async || {
            calls.set(calls.get() + 1);
            calls.get() == 4
        };
        let started = Instant::now();
        check::eventually_true(&seat, Duration::from_secs(1), fourth, "c").await;
        assert!(!seat.failed(), "{}", seat.message());
        assert_eq!(started.elapsed(), Duration::from_millis(1 + 2 + 4));
    }

    #[tokio::test(start_paused = true)]
    async fn caps_the_backoff_at_a_quarter_of_the_timeout() {
        let seat = Recorder::new();
        let calls = Cell::new(0);
        let sixth = async || {
            calls.set(calls.get() + 1);
            calls.get() == 6
        };
        let started = Instant::now();
        check::eventually_true(&seat, Duration::from_millis(16), sixth, "c").await;
        assert!(!seat.failed(), "{}", seat.message());
        assert_eq!(started.elapsed(), Duration::from_millis(1 + 2 + 4 + 4 + 4));
    }

    #[tokio::test(start_paused = true)]
    async fn states_the_attempts_of_a_predicate_that_stays_false() {
        let seat = Recorder::new();
        soft::eventually_true(&seat, Duration::from_millis(16), async || false, "c").await;
        let failure = &seat.failures()[0];
        assert_eq!(failure.assertion(), Some("eventually-true"));
        assert_eq!(failure.detail(), [("attempts", Value::Int(7))]);
    }
}

mod no_task_leaks {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn passes_a_body_that_joins_its_tasks() {
        let seat = Recorder::new();
        let joins = async || tokio::spawn(async {}).await.expect("the task ends");
        check::no_task_leaks(&seat, joins, "c").await;
        assert!(!seat.failed(), "{}", seat.message());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn passes_a_body_that_joins_its_tasks_on_the_multi_thread_runtime() {
        let seat = Recorder::new();
        let joins = async || {
            let tasks: Vec<_> = (0..8).map(|_| tokio::spawn(async {})).collect();
            for task in tasks {
                task.await.expect("the task ends");
            }
        };
        check::no_task_leaks(&seat, joins, "c").await;
        assert!(!seat.failed(), "{}", seat.message());
    }

    #[tokio::test(start_paused = true)]
    async fn passes_a_task_that_ends_within_half_a_second() {
        let seat = Recorder::new();
        let lingers = async || drop(tokio::spawn(tokio::time::sleep(Duration::from_millis(400))));
        check::no_task_leaks(&seat, lingers, "c").await;
        assert!(!seat.failed(), "{}", seat.message());
    }

    #[tokio::test(start_paused = true)]
    async fn states_the_number_of_tasks_that_run_after_half_a_second() {
        let seat = Recorder::new();
        let leaks = async || {
            drop(tokio::spawn(tokio::time::sleep(Duration::from_millis(600))));
            drop(tokio::spawn(std::future::pending::<()>()));
        };
        check::no_task_leaks(&seat, leaks, "c").await;
        let failure = &seat.failures()[0];
        assert_eq!(failure.assertion(), Some("no-task-leaks"));
        assert_eq!(failure.detail(), [("leaked", Value::Int(2))]);
    }

    #[tokio::test(start_paused = true)]
    async fn counts_no_task_that_ran_before_the_body() {
        let seat = Recorder::new();
        drop(tokio::spawn(std::future::pending::<()>()));
        check::no_task_leaks(&seat, async || {}, "c").await;
        assert!(!seat.failed(), "{}", seat.message());
    }

    #[test]
    fn ends_with_a_fault_outside_a_tokio_runtime() {
        let seat = Recorder::new();
        let ran = Cell::new(false);
        block_on(check::no_task_leaks(&seat, async || ran.set(true), "c"));
        assert_eq!(
            seat.message(),
            "no_task_leaks counts the tasks of a tokio runtime, and the test does not run on one"
        );
        assert_eq!(records(&seat)[0]["verdict"], "error");
        assert!(!ran.get());
    }
}
