//! The assertions whose subject is a future.
//!
//! Each is driven twice, with a subject that satisfies it and one that does not. Both
//! halves are needed here more than anywhere: dropping a future stops it whether or not
//! it cooperated, so an assertion that only checked the subject stopped would report
//! nothing whatever it was handed. That bug has shipped in three other languages.
//!
//! Naming each assertion is also what proves it exists: Rust cannot look a function up by
//! name at run time, so a rename fails this build rather than a test.

use dokimi_assert::matcher::behaviour::Stop;
use dokimi_assert::seat::{Recorder, Seat};
use dokimi_assert_tokio::check;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn honours_cancellation_passes_a_subject_that_reads_the_token() {
    let ran = Arc::new(AtomicBool::new(false));
    let saw = Arc::clone(&ran);

    let seat = Recorder::new();
    check::honours_cancellation(
        &seat,
        move |token: CancellationToken| async move {
            saw.store(true, Ordering::SeqCst);
            if token.is_cancelled() {
                return Err(Stop::Cancelled);
            }
            Ok(())
        },
        "the worker stops when told",
    )
    .await;

    assert!(!seat.failed(), "{}", seat.message());
    assert!(
        ran.load(Ordering::SeqCst),
        "the subject has to run, or passing means nothing"
    );
}

#[tokio::test]
async fn honours_cancellation_reports_a_subject_that_ignores_the_token() {
    let seat = Recorder::new();
    check::honours_cancellation(
        &seat,
        |_token: CancellationToken| async move { Ok::<(), Stop>(()) },
        "the worker stops when told",
    )
    .await;

    assert!(
        seat.failed(),
        "a subject that never reads the token must be reported"
    );
}

#[tokio::test]
async fn honours_cancellation_reports_a_subject_that_fails_for_its_own_reasons() {
    // Failing and happening to do so in time is not honouring the token.
    let seat = Recorder::new();
    check::honours_cancellation(
        &seat,
        |_token: CancellationToken| async move { Err::<(), Stop>(Stop::DeadlineExceeded) },
        "the worker stops when told",
    )
    .await;

    assert!(seat.failed(), "the wrong reason is not the right reason");
}

#[tokio::test]
async fn honours_deadline_asks_for_the_other_reason() {
    let seat = Recorder::new();
    check::honours_deadline(
        &seat,
        |_token: CancellationToken| async move { Err::<(), Stop>(Stop::DeadlineExceeded) },
        "the worker respects no time",
    )
    .await;
    assert!(!seat.failed(), "{}", seat.message());

    let wrong = Recorder::new();
    check::honours_deadline(
        &wrong,
        |_token: CancellationToken| async move { Err::<(), Stop>(Stop::Cancelled) },
        "the worker respects no time",
    )
    .await;
    assert!(
        wrong.failed(),
        "a subject that reports the other reason must be reported"
    );
}

#[tokio::test]
async fn completes_within_passes_a_quick_future_and_reports_a_slow_one() {
    let quick = Recorder::new();
    check::completes_within(&quick, Duration::from_secs(10), async {}, "get stays quick").await;
    assert!(!quick.failed(), "{}", quick.message());

    let slow = Recorder::new();
    check::completes_within(
        &slow,
        Duration::from_millis(10),
        tokio::time::sleep(Duration::from_secs(30)),
        "get stays quick",
    )
    .await;
    assert!(slow.failed(), "a future over the ceiling must be reported");
}

#[tokio::test]
async fn eventually_passes_once_the_body_settles() {
    let attempts = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counted = Arc::clone(&attempts);

    let seat = Recorder::new();
    check::eventually(
        &seat,
        Duration::from_millis(500),
        Duration::from_millis(1),
        move |trial: Recorder| {
            let counted = Arc::clone(&counted);
            async move {
                if counted.fetch_add(1, Ordering::SeqCst) < 2 {
                    Seat::fail(&trial, "not ready yet");
                }
                trial
            }
        },
        "the queue drains",
    )
    .await;

    assert!(!seat.failed(), "{}", seat.message());
    assert!(
        attempts.load(Ordering::SeqCst) >= 3,
        "the body has to be retried"
    );
}

#[tokio::test]
async fn eventually_carries_the_last_attempts_reason() {
    let seat = Recorder::new();
    check::eventually(
        &seat,
        Duration::from_millis(20),
        Duration::from_millis(1),
        |trial: Recorder| async move {
            Seat::fail(&trial, "queue still holds 4");
            trial
        },
        "the queue drains",
    )
    .await;

    assert!(seat.failed(), "a body that never passes must be reported");
    assert!(
        seat.message().contains("queue still holds 4"),
        "{}",
        seat.message()
    );
}

#[tokio::test]
async fn eventually_true_passes_a_predicate_that_flips_and_reports_one_that_does_not() {
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counted = Arc::clone(&calls);

    let seat = Recorder::new();
    check::eventually_true(
        &seat,
        Duration::from_millis(500),
        move || {
            let counted = Arc::clone(&counted);
            async move { counted.fetch_add(1, Ordering::SeqCst) >= 2 }
        },
        "the file appears",
    )
    .await;
    assert!(!seat.failed(), "{}", seat.message());
    assert!(
        calls.load(Ordering::SeqCst) >= 3,
        "the predicate has to be retried"
    );

    let never = Recorder::new();
    check::eventually_true(
        &never,
        Duration::from_millis(20),
        || async { false },
        "the file appears",
    )
    .await;
    assert!(
        never.failed(),
        "a predicate that never holds must be reported"
    );
    assert!(
        never.message().contains("still false"),
        "{}",
        never.message()
    );
}

#[tokio::test]
async fn no_task_leaks_passes_a_scope_that_leaves_nothing_behind() {
    let seat = Recorder::new();
    check::no_task_leaks(
        &seat,
        || async {
            let joined = tokio::spawn(async { 1 + 1 });
            let _ = joined.await;
        },
        "handle starts nothing of its own",
    )
    .await;

    assert!(!seat.failed(), "{}", seat.message());
}

#[tokio::test]
async fn no_task_leaks_reports_a_task_left_running() {
    let token = CancellationToken::new();
    let held = token.clone();

    let seat = Recorder::new();
    check::no_task_leaks(
        &seat,
        || async {
            // Spawned and never awaited, which is the leak.
            tokio::spawn(async move { held.cancelled().await });
        },
        "handle starts nothing of its own",
    )
    .await;

    assert!(seat.failed(), "work outliving the scope must be reported");
    assert!(
        seat.message().contains("still running"),
        "{}",
        seat.message()
    );
    token.cancel();
}
