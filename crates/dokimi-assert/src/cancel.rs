//! The handle that tells a subject to stop.

use std::error::Error;
use std::fmt::{self, Debug, Display};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::task::{Context, Poll, Waker};

/// Why a subject was told to stop.
///
/// A subject that reads a stopped [`Cancel`] returns the reason as its error,
/// or as the source of its error. [`check::honours_cancellation`] and
/// [`check::honours_deadline`] look for it in the chain of sources.
///
/// [`check::honours_cancellation`]: crate::check::honours_cancellation
/// [`check::honours_deadline`]: crate::check::honours_deadline
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Stop {
    /// The caller gave up.
    Cancelled,
    /// The time that the caller allowed ran out.
    DeadlineExceeded,
}

impl Display for Stop {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Cancelled => "cancelled",
            Self::DeadlineExceeded => "deadline exceeded",
        })
    }
}

impl Error for Stop {}

/// A handle that a subject reads to learn that it should stop, and why.
///
/// The handle plays the part of Go's `context.Context` and of tokio-util's
/// `CancellationToken`. Clones share one state, so a test keeps one clone
/// and hands another to the subject. A handle that [`Cancel::child`]
/// derives stops when its parent stops, and it can stop on its own.
///
/// Synchronous code reads [`Cancel::reason`]. Asynchronous code awaits
/// [`Cancel::stopped`], which works on any executor.
///
/// ```
/// use dokimi_assert::{Cancel, Stop};
///
/// fn fetch(cancel: &Cancel) -> Result<String, Stop> {
///     if let Some(reason) = cancel.reason() {
///         return Err(reason);
///     }
///     Ok("fetched".to_owned())
/// }
///
/// let parent = Cancel::new();
/// let child = parent.child();
/// parent.cancel();
/// assert_eq!(fetch(&child), Err(Stop::Cancelled));
/// ```
#[derive(Clone, Default)]
pub struct Cancel {
    inner: Arc<Inner>,
}

#[derive(Default)]
struct Inner {
    state: AtomicU8,
    parent: Option<Cancel>,
    /// The tasks that await the handle, or a handle that derives from it.
    wakers: Mutex<Vec<Waker>>,
}

impl Inner {
    /// Registers `waker` to be woken when the handle stops.
    fn register(&self, waker: &Waker) {
        let mut wakers = self.wakers.lock().unwrap_or_else(PoisonError::into_inner);
        if !wakers.iter().any(|held| held.will_wake(waker)) {
            wakers.push(waker.clone());
        }
    }
}

const RUNNING: u8 = 0;
const CANCELLED: u8 = 1;
const EXPIRED: u8 = 2;

impl Cancel {
    /// Returns a handle that has not stopped.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns a handle that the caller has already cancelled.
    #[must_use]
    pub fn cancelled() -> Self {
        let handle = Self::new();
        handle.cancel();
        handle
    }

    /// Returns a handle whose deadline has already passed.
    #[must_use]
    pub fn expired() -> Self {
        let handle = Self::new();
        handle.expire();
        handle
    }

    /// Returns a handle that stops when this handle stops, with this
    /// handle's reason.
    ///
    /// Stopping the child does not stop this handle.
    #[must_use]
    pub fn child(&self) -> Self {
        Self {
            inner: Arc::new(Inner {
                parent: Some(self.clone()),
                ..Inner::default()
            }),
        }
    }

    /// Stops the handle because the caller gave up.
    ///
    /// A handle that has already stopped keeps its first reason.
    pub fn cancel(&self) {
        self.stop(CANCELLED);
    }

    /// Stops the handle because its deadline passed.
    ///
    /// A handle that has already stopped keeps its first reason.
    pub fn expire(&self) {
        self.stop(EXPIRED);
    }

    fn stop(&self, reason: u8) {
        let first = self
            .inner
            .state
            .compare_exchange(RUNNING, reason, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok();
        if first {
            let wakers = std::mem::take(
                &mut *self
                    .inner
                    .wakers
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner),
            );
            wakers.into_iter().for_each(Waker::wake);
        }
    }

    /// Returns why the handle stopped, or `None` while it runs.
    ///
    /// A derived handle that has not stopped on its own returns the reason of
    /// its parent.
    #[must_use]
    pub fn reason(&self) -> Option<Stop> {
        match self.inner.state.load(Ordering::SeqCst) {
            CANCELLED => Some(Stop::Cancelled),
            EXPIRED => Some(Stop::DeadlineExceeded),
            _ => self.inner.parent.as_ref().and_then(Self::reason),
        }
    }

    /// Returns `true` when the handle has stopped, for either reason.
    #[must_use]
    pub fn is_stopped(&self) -> bool {
        self.reason().is_some()
    }

    /// Returns a future that completes with the reason when the handle
    /// stops, or when a handle that it derives from stops.
    ///
    /// The future needs no particular runtime. An asynchronous subject races
    /// it against its work, as it would race tokio-util's
    /// `CancellationToken::cancelled`.
    pub fn stopped(&self) -> Stopped {
        Stopped {
            handle: self.clone(),
        }
    }
}

impl Debug for Cancel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Cancel")
            .field("reason", &self.reason())
            .finish_non_exhaustive()
    }
}

/// The future that [`Cancel::stopped`] returns.
#[derive(Debug, Clone)]
#[must_use = "a future does nothing unless it is awaited"]
pub struct Stopped {
    handle: Cancel,
}

impl Future for Stopped {
    type Output = Stop;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Stop> {
        if let Some(reason) = self.handle.reason() {
            return Poll::Ready(reason);
        }
        let mut link = Some(&self.handle);
        while let Some(handle) = link {
            handle.inner.register(cx.waker());
            link = handle.inner.parent.as_ref();
        }
        // A stop between the first reading and the registration woke no
        // waker of this task, so the reason is read once more.
        self.handle.reason().map_or(Poll::Pending, Poll::Ready)
    }
}
