//! How a subject behaves, rather than what it answers.
//!
//! Go states cancellation with a `context.Context` in every signature, and Rust has no
//! such value in its standard library. Dropping a future is not the equivalent: a subject
//! that stops because it was dropped never chose to stop, so asking whether it honours
//! cancellation would answer yes for every subject, which is the shape this assertion has
//! shipped broken in three other languages.
//!
//! So the handle is a value the subject is given and can read, which is the question the
//! standard actually asks. [`Cancel`] is that value.

use super::errors::error_as;
use super::report::{Mode, fail};
use crate::seat::{Recorder, Seat};
use std::error::Error;
use std::fmt::{self, Debug, Display};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::Duration;

/// Why a subject was asked to stop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stop {
    /// The caller gave up.
    Cancelled,
    /// The time allowed ran out.
    DeadlineExceeded,
}

impl Display for Stop {
    #[track_caller]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => write!(f, "cancelled"),
            Self::DeadlineExceeded => write!(f, "deadline exceeded"),
        }
    }
}

impl Error for Stop {}

/// A handle a subject reads to learn it should stop.
///
/// The smallest thing that answers the question `context.Context` answers in Go: whether
/// the caller has given up, and why. A subject that can be cancelled at all takes one.
#[derive(Debug, Default)]
pub struct Cancel {
    state: AtomicU8,
}

const RUNNING: u8 = 0;
const CANCELLED: u8 = 1;
const EXPIRED: u8 = 2;

impl Cancel {
    /// Return a handle that has not been stopped.
    #[must_use]
    #[track_caller]
    pub fn new() -> Self {
        Self::default()
    }

    /// Return a handle the caller has already given up on.
    #[must_use]
    #[track_caller]
    pub fn cancelled() -> Self {
        let held = Self::new();
        held.state.store(CANCELLED, Ordering::SeqCst);
        held
    }

    /// Return a handle whose time has already run out.
    #[must_use]
    #[track_caller]
    pub fn expired() -> Self {
        let held = Self::new();
        held.state.store(EXPIRED, Ordering::SeqCst);
        held
    }

    /// Ask whatever holds this to stop, as a caller giving up.
    #[track_caller]
    pub fn stop(&self) {
        let _ = self
            .state
            .compare_exchange(RUNNING, CANCELLED, Ordering::SeqCst, Ordering::SeqCst);
    }

    /// Ask whatever holds this to stop, because the time ran out.
    #[track_caller]
    pub fn expire(&self) {
        let _ = self
            .state
            .compare_exchange(RUNNING, EXPIRED, Ordering::SeqCst, Ordering::SeqCst);
    }

    /// Why this was stopped, or `None` while it is still running.
    ///
    /// The counterpart of `ctx.Err()`: a subject polls it and returns when it answers.
    #[must_use]
    #[track_caller]
    pub fn stopped(&self) -> Option<Stop> {
        match self.state.load(Ordering::SeqCst) {
            CANCELLED => Some(Stop::Cancelled),
            EXPIRED => Some(Stop::DeadlineExceeded),
            _ => None,
        }
    }

    /// Whether this has been stopped at all.
    #[must_use]
    #[track_caller]
    pub fn is_stopped(&self) -> bool {
        self.stopped().is_some()
    }
}

/// Report when a subject given a stopped handle does not say it stopped for that reason.
#[track_caller]
fn honours<E, F>(seat: &dyn Seat, mode: Mode, handle: &Cancel, want: Stop, body: F, msg: &str)
where
    E: Error + 'static,
    F: FnOnce(Option<&Cancel>) -> Result<(), E>,
{
    match body(Some(handle)) {
        Ok(()) => {
            fail(
                seat,
                mode,
                assertion_for(want),
                msg,
                vec![(
                    "got",
                    "returned as if it had done the work".to_owned().into(),
                )],
            );
        }
        Err(raised) => {
            // Its own failure that happened to arrive in time is not the
            // same as honouring the handle, so the reason is checked.
            let quiet = Recorder::new();
            let found = error_as::<Stop>(&quiet, Mode::Soft, &raised, msg);
            match found {
                Some(&got) if got == want => {}
                _ => fail(
                    seat,
                    mode,
                    assertion_for(want),
                    msg,
                    vec![("got", format!("{raised}").into())],
                ),
            }
        }
    }
}

/// Report when a subject told to stop does not say so.
///
/// The subject is handed a handle that has already been cancelled, so this asks whether
/// it reads the handle at all rather than how quickly it notices. A subject that ignores
/// it does the work and answers `Ok`, which fails here.
#[track_caller]
pub fn honours_cancellation<E, F>(seat: &dyn Seat, mode: Mode, body: F, msg: &str)
where
    E: Error + 'static,
    F: FnOnce(Option<&Cancel>) -> Result<(), E>,
{
    seat.helper();
    honours(seat, mode, &Cancel::cancelled(), Stop::Cancelled, body, msg);
}

/// Report when a subject given no time does not say its deadline passed.
///
/// This differs from [`honours_cancellation`] in which reason it asks for: a subject may
/// distinguish a caller who gave up from one who ran out of time.
#[track_caller]
pub fn honours_deadline<E, F>(seat: &dyn Seat, mode: Mode, body: F, msg: &str)
where
    E: Error + 'static,
    F: FnOnce(Option<&Cancel>) -> Result<(), E>,
{
    seat.helper();
    honours(
        seat,
        mode,
        &Cancel::expired(),
        Stop::DeadlineExceeded,
        body,
        msg,
    );
}

/// Report when a subject given no handle at all panics.
///
/// Answering an error of its own is fine and is usually right. What fails here is
/// panicking on the missing handle, which is what a caller hits by accident.
#[track_caller]
pub fn none_handle_safe<E, F>(seat: &dyn Seat, mode: Mode, body: F, msg: &str)
where
    E: Debug,
    F: FnOnce(Option<&Cancel>) -> Result<(), E> + std::panic::UnwindSafe,
{
    seat.helper();
    if super::panics::quietly(|| drop(body(None))) {
        fail(
            seat,
            mode,
            "nil-context-safe",
            msg,
            vec![("got", "a panic".to_owned().into())],
        );
    }
}

/// Report when the body takes longer than the given duration.
///
/// The body is given a handle that is stopped once the time is up, so a subject that
/// reads it can give up. It is measured either way: one that runs long runs to completion
/// and then fails. This spends real time, up to however long the body takes.
#[track_caller]
pub fn completes_within<E, F>(seat: &dyn Seat, mode: Mode, within: Duration, body: F, msg: &str)
where
    E: Debug,
    F: FnOnce(Option<&Cancel>) -> Result<(), E>,
{
    seat.helper();
    let handle = Arc::new(Cancel::new());
    let watcher = Arc::clone(&handle);

    // The watcher waits on a condition rather than sleeping, so a body
    // that returns early ends the wait rather than leaving it to run out.
    // Sleeping here would make every call take the whole ceiling, which
    // is the opposite of what an assertion about promptness should cost.
    let signal = Arc::new((Mutex::new(false), Condvar::new()));
    let waited = Arc::clone(&signal);

    let deadline = std::thread::spawn(move || {
        let (lock, wake) = &*waited;
        let done = lock.lock().unwrap_or_else(PoisonError::into_inner);
        let (done, timed_out) = wake
            .wait_timeout_while(done, within, |done| !*done)
            .unwrap_or_else(PoisonError::into_inner);
        drop(done);
        if timed_out.timed_out() {
            watcher.expire();
        }
    });

    // The verdict is read from the seat's clock, as every other timed
    // assertion here is, so a test driving a controlled clock decides
    // what the subject took. The watcher above expires on the platform
    // clock, because a condition variable takes no other.
    let clock = seat.clock();
    let started = clock.now();
    let _ = body(Some(&handle));
    let elapsed = clock.now().saturating_sub(started);

    {
        let (lock, wake) = &*signal;
        *lock.lock().unwrap_or_else(PoisonError::into_inner) = true;
        wake.notify_all();
    }
    let _ = deadline.join();

    if elapsed > within {
        fail(
            seat,
            mode,
            "completes-within",
            msg,
            vec![
                ("want", format!("{}ms", within.as_millis()).into()),
                ("got", format!("{}ms", elapsed.as_millis()).into()),
            ],
        );
    }
}

/// Report when the body changes what observe reads.
///
/// What observe answers defines what nothing means: whatever it leaves out, the body is
/// free to change. Answering an owned value is what makes the reading a snapshot; a
/// borrow of the subject would read the same memory twice and pass whatever the body did.
#[track_caller]
pub fn is_pure<S, O, F>(seat: &dyn Seat, mode: Mode, observe: O, body: F, msg: &str)
where
    S: PartialEq + Debug,
    O: Fn() -> S,
    F: FnOnce(),
{
    seat.helper();
    let before = observe();
    body();
    let after = observe();

    if before != after {
        fail(
            seat,
            mode,
            "pure",
            msg,
            vec![
                ("want", format!("{before:?}").into()),
                ("got", format!("{after:?}").into()),
            ],
        );
    }
}

/// Report when the body does not report a failure.
///
/// The body is handed a [`Recorder`], so assertions inside it record instead of ending
/// the test. A body that passes is the failure. Yields the failure the body produced, so
/// its text can be asserted on.
#[track_caller]
pub fn rejects<F>(seat: &dyn Seat, mode: Mode, body: F, msg: &str) -> String
where
    F: FnOnce(&Recorder),
{
    seat.helper();
    let recorder = Recorder::new();
    body(&recorder);

    if recorder.failed() {
        return recorder.message();
    }
    fail(seat, mode, "rejects", msg, vec![]);
    String::new()
}

/// The canonical id a stopped-handle assertion reports under.
///
/// One matcher serves both, because the only difference is which reason the subject has
/// to give; the failure still names the assertion the caller wrote.
const fn assertion_for(want: Stop) -> &'static str {
    match want {
        Stop::Cancelled => "honours-cancellation",
        Stop::DeadlineExceeded => "honours-deadline",
    }
}
