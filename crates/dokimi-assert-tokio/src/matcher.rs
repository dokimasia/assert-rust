// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The comparisons of the assertions, which both surfaces call with the mode
//! of their surface.

use dokimi_assert::__private::{Body, Mode, Run, chain, message, nanoseconds, quietly, waited};
use dokimi_assert::{Cancel, Seat, Stop, Value};
use std::error::Error;
use std::future::Future;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::runtime::{Handle, RuntimeMetrics};
use tokio::time::Instant;

/// The shortest wait between two attempts of a retrying assertion.
const MIN_WAIT: Duration = Duration::from_millis(1);

/// The number of readings of the task count after the body of
/// `no_task_leaks`.
const LEAK_READINGS: usize = 100;

/// The wait between two readings of the task count. 100 readings span half a
/// second of tokio's clock.
const LEAK_INTERVAL: Duration = Duration::from_millis(5);

/// Whether the panic hook prints a panic of a future that [`Caught`] wraps.
#[derive(Debug, Clone, Copy)]
enum Hook {
    /// The hook prints the panic, as it prints a panic that no assertion
    /// expects.
    Prints,
    /// The hook prints nothing, because the assertion states the panic.
    Silent,
}

/// A future that completes with the outcome of the future that it wraps: the
/// value, or the payload of the panic.
struct Caught<F> {
    future: Pin<Box<F>>,
    hook: Hook,
}

impl<F: Future> Future for Caught<F> {
    type Output = std::thread::Result<F::Output>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let hook = self.hook;
        let future = self.future.as_mut();
        let polled = match hook {
            Hook::Prints => catch_unwind(AssertUnwindSafe(|| future.poll(cx))),
            Hook::Silent => quietly(|| future.poll(cx)),
        };
        match polled {
            Ok(Poll::Pending) => Poll::Pending,
            Ok(Poll::Ready(value)) => Poll::Ready(Ok(value)),
            Err(payload) => Poll::Ready(Err(payload)),
        }
    }
}

/// Returns a future that completes with the outcome of `future`.
fn caught<F: Future>(future: F, hook: Hook) -> Caught<F> {
    Caught {
        future: Box::pin(future),
        hook,
    }
}

/// Hands `body` a handle that stopped for `reason`, and reports whether the
/// body returned an error whose chain contains that reason. The failure
/// states the error that it returned instead, the message of its panic, or
/// null when it returned `Ok`.
pub(crate) async fn honours<E: Error + 'static>(
    run: Run<'_>,
    mode: Mode,
    assertion: &'static str,
    reason: Stop,
    body: impl AsyncFnOnce(Option<&Cancel>) -> Result<(), E>,
    msg: &str,
) {
    let handle = match reason {
        Stop::Cancelled => Cancel::cancelled(),
        Stop::DeadlineExceeded => Cancel::expired(),
    };
    let got = match caught(body(Some(&handle)), Hook::Silent).await {
        Ok(Ok(())) => Value::Null,
        Ok(Err(error)) => {
            if chain(&error)
                .iter()
                .any(|link| link.downcast_ref::<Stop>() == Some(&reason))
            {
                run.pass(mode, assertion, msg);
                return;
            }
            Value::Opaque(format!("{error:?}"))
        }
        Err(payload) => Value::from(message(&*payload)),
    };
    run.fail(mode, assertion, msg, vec![("got", got)]);
}

/// Reports whether `body` completes within `within` on tokio's clock. A body
/// that has not completed by then is dropped, and the failure states the time
/// waited.
pub(crate) async fn completes_within(
    run: Run<'_>,
    mode: Mode,
    within: Duration,
    body: impl Future,
    msg: &str,
) {
    let started = Instant::now();
    if tokio::time::timeout(within, body).await.is_ok() {
        run.pass(mode, "completes-within", msg);
        return;
    }
    let detail = vec![
        ("want", nanoseconds(within)),
        ("got", waited(started.elapsed())),
    ];
    run.fail(mode, "completes-within", msg, detail);
}

/// Waits `duration` on tokio's clock, or the time left before `deadline` when
/// that is less, and returns `true` when another attempt may start: the clock
/// read before the deadline, and after the wait it reads no later than it.
async fn wait_within(deadline: Instant, duration: Duration) -> bool {
    let now = Instant::now();
    if now >= deadline {
        return false;
    }
    tokio::time::sleep(duration.min(deadline.saturating_duration_since(now))).await;
    Instant::now() <= deadline
}

/// Reports whether `body` passes within `timeout`, retried every `interval`
/// on tokio's clock.
///
/// Each attempt runs on a seat of its own, as the attempts of the synchronous
/// `eventually` run. The body runs at least once, and an attempt passes the
/// call only when it ends by the deadline.
pub(crate) async fn eventually(
    run: Run<'_>,
    seat: &dyn Seat,
    mode: Mode,
    timeout: Duration,
    interval: Duration,
    body: impl AsyncFn(&dyn Seat),
    msg: &str,
) {
    let deadline = Instant::now() + timeout;
    for attempts in 1_usize.. {
        let calls = run.body_calls();
        let attempt = Body::new(seat, calls.clone());
        attempt.finish(caught(body(&attempt), Hook::Prints).await);
        run.take(calls.as_ref());
        if !attempt.failed() && Instant::now() <= deadline {
            run.pass(mode, "eventually", msg);
            return;
        }
        if !wait_within(deadline, interval.max(MIN_WAIT)).await {
            let detail = vec![
                ("attempts", Value::from(attempts)),
                ("last", Value::from(attempt.first_message())),
            ];
            run.fail(mode, "eventually", msg, detail);
            return;
        }
    }
}

/// Reports whether `predicate` becomes true within `timeout`, retried with a
/// backoff that starts at a millisecond and doubles, up to a quarter of the
/// timeout.
pub(crate) async fn eventually_true(
    run: Run<'_>,
    mode: Mode,
    timeout: Duration,
    predicate: impl AsyncFn() -> bool,
    msg: &str,
) {
    let deadline = Instant::now() + timeout;
    let cap = (timeout / 4).max(MIN_WAIT);
    let mut backoff = MIN_WAIT;
    for attempts in 1_usize.. {
        if predicate().await && Instant::now() <= deadline {
            run.pass(mode, "eventually-true", msg);
            return;
        }
        if !wait_within(deadline, backoff).await {
            run.fail(
                mode,
                "eventually-true",
                msg,
                vec![("attempts", Value::from(attempts))],
            );
            return;
        }
        backoff = (backoff * 2).min(cap);
    }
}

/// Reports whether every task that `body` spawned on the runtime has ended
/// once the body completes. The failure states the number of tasks that
/// still run.
pub(crate) async fn no_task_leaks(run: Run<'_>, mode: Mode, body: impl AsyncFnOnce(), msg: &str) {
    let Ok(runtime) = Handle::try_current() else {
        let text =
            "no_task_leaks counts the tasks of a tokio runtime, and the test does not run on one";
        run.fault(mode, "no-task-leaks", msg, text);
        return;
    };
    let metrics = runtime.metrics();
    let before = metrics.num_alive_tasks();
    body().await;
    let leaked = still_alive(&metrics, before).await;
    if leaked > 0 {
        run.fail(
            mode,
            "no-task-leaks",
            msg,
            vec![("leaked", Value::from(leaked))],
        );
        return;
    }
    run.pass(mode, "no-task-leaks", msg);
}

/// Returns the number of live tasks above `before`, read up to 100 times, 5 ms
/// apart on tokio's clock. It returns 0 as soon as a reading finds no more
/// tasks than `before`.
///
/// A task that ends during that half second is not a leak. The readings also
/// cover the count of the multi-thread runtime, which tokio states may lag
/// behind a task that ended.
async fn still_alive(metrics: &RuntimeMetrics, before: usize) -> usize {
    for _ in 1..LEAK_READINGS {
        if metrics.num_alive_tasks() <= before {
            return 0;
        }
        tokio::time::sleep(LEAK_INTERVAL).await;
    }
    metrics.num_alive_tasks().saturating_sub(before)
}
