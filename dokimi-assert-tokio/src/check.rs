//! The assertions whose subject is a future, and the one that counts tasks.
//!
//! These mirror the synchronous crate's members under the same names. A caller uses the
//! synchronous crate for everything else, including every comparison.

use dokimi_assert::matcher::behaviour::Stop;
use dokimi_assert::matcher::{Mode, report};
use dokimi_assert::seat::{Recorder, Seat};
use std::error::Error;
use std::future::Future;
use std::time::Duration;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

/// Report when a subject given a stopped token does not say it stopped for that reason.
async fn honours<E, F, Fut>(seat: &dyn Seat, mode: Mode, want: Stop, body: F, msg: &str)
where
    E: Error + 'static,
    F: FnOnce(CancellationToken) -> Fut,
    Fut: Future<Output = Result<(), E>>,
{
    let token = CancellationToken::new();
    token.cancel();

    match body(token).await {
        Ok(()) => report(
            seat,
            mode,
            &format!("{msg}: a subject given a {want} token completed as if it had done the work"),
        ),
        Err(raised) => {
            // Dropping a future stops it whatever it wanted, so stopping
            // proves nothing on its own. The reason is what is checked.
            let quiet = Recorder::new();
            let found =
                dokimi_assert::matcher::errors::error_as::<Stop>(&quiet, Mode::Soft, &raised, msg);
            if found != Some(&want) {
                report(
                    seat,
                    mode,
                    &format!("{msg}: a subject given a {want} token failed with {raised} instead"),
                );
            }
        }
    }
}

/// Fail when a subject told to stop does not say so.
///
/// The subject is handed an already-cancelled [`CancellationToken`] and must answer with
/// [`Stop::Cancelled`] in its error chain. Simply not finishing is not enough: dropping a
/// future cancels it whether or not it cooperated, so a test that only checked the
/// subject stopped would pass for every subject.
pub async fn honours_cancellation<E, F, Fut>(seat: &dyn Seat, body: F, msg: &str)
where
    E: Error + 'static,
    F: FnOnce(CancellationToken) -> Fut,
    Fut: Future<Output = Result<(), E>>,
{
    seat.helper();
    honours(seat, Mode::Fatal, Stop::Cancelled, body, msg).await;
}

/// Fail when a subject given no time does not say its deadline passed.
pub async fn honours_deadline<E, F, Fut>(seat: &dyn Seat, body: F, msg: &str)
where
    E: Error + 'static,
    F: FnOnce(CancellationToken) -> Fut,
    Fut: Future<Output = Result<(), E>>,
{
    seat.helper();
    honours(seat, Mode::Fatal, Stop::DeadlineExceeded, body, msg).await;
}

/// Fail when the future does not finish within the given duration.
///
/// The future is dropped once the time is up rather than left running, which is how a
/// caller would give up on it.
pub async fn completes_within<T, Fut>(seat: &dyn Seat, within: Duration, body: Fut, msg: &str)
where
    Fut: Future<Output = T>,
{
    seat.helper();
    let started = Instant::now();
    if tokio::time::timeout(within, body).await.is_err() {
        report(
            seat,
            Mode::Fatal,
            &format!(
                "{msg}: still running after {}ms, want at most the same",
                started.elapsed().as_millis()
            ),
        );
    }
}

/// Fail when a body of assertions never passes within the timeout.
///
/// The body is handed a seat of its own, so assertions inside it record an attempt rather
/// than ending the test. It runs at least once however short the timeout.
pub async fn eventually<F, Fut>(
    seat: &dyn Seat,
    timeout: Duration,
    interval: Duration,
    body: F,
    msg: &str,
) where
    F: Fn(Recorder) -> Fut,
    Fut: Future<Output = Recorder>,
{
    seat.helper();
    let deadline = Instant::now() + timeout;

    for attempt in 1.. {
        let trial = body(Recorder::new()).await;
        if !trial.failed() {
            return;
        }
        if Instant::now() > deadline {
            report(
                seat,
                Mode::Fatal,
                &format!(
                    "{msg}: still failing after {}ms and {attempt} attempts: {}",
                    timeout.as_millis(),
                    trial.message()
                ),
            );
            return;
        }
        tokio::time::sleep(interval.max(Duration::from_millis(1))).await;
    }
}

/// Fail when a predicate never becomes true within the timeout.
///
/// Retried with a backoff that doubles, capped at a quarter of the timeout.
pub async fn eventually_true<P, Fut>(seat: &dyn Seat, timeout: Duration, predicate: P, msg: &str)
where
    P: Fn() -> Fut,
    Fut: Future<Output = bool>,
{
    seat.helper();
    let deadline = Instant::now() + timeout;
    let cap = (timeout / 4).max(Duration::from_millis(1));
    let mut backoff = Duration::from_millis(1);

    for attempt in 1.. {
        if predicate().await {
            return;
        }
        if Instant::now() > deadline {
            report(
                seat,
                Mode::Fatal,
                &format!(
                    "{msg}: still false after {}ms and {attempt} attempts",
                    timeout.as_millis()
                ),
            );
            return;
        }
        tokio::time::sleep(backoff).await;
        backoff = (backoff * 2).min(cap);
    }
}

/// Fail when a task spawned inside the body outlives it.
///
/// Reads the runtime's count of live tasks either side of the body. This is why the
/// assertion lives here rather than in the synchronous crate: Rust's standard library has
/// no way to enumerate threads, so nothing there can answer the question at all.
///
/// A spawned task is not joined by being dropped, so a body that spawns and returns
/// without awaiting is what this catches.
///
/// # Panics
///
/// When called outside a Tokio runtime, since there is no task count without one.
pub async fn no_task_leaks<F, Fut>(seat: &dyn Seat, body: F, msg: &str)
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = ()>,
{
    seat.helper();
    let metrics = tokio::runtime::Handle::current().metrics();
    let before = metrics.num_alive_tasks();

    body().await;

    // A task that finished may take a moment to leave the count, and a
    // leaked one never will. Give the finished ones that moment.
    tokio::task::yield_now().await;
    let after = metrics.num_alive_tasks();

    if after > before {
        report(
            seat,
            Mode::Fatal,
            &format!("{msg}: {} task(s) still running", after - before),
        );
    }
}
