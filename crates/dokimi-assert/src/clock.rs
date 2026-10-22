//! The clocks that the timed assertions read.

use std::fmt::Debug;
use std::sync::{Condvar, LazyLock, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

/// The instant at which the process first read the shared platform clock.
static ORIGIN: LazyLock<Instant> = LazyLock::new(Instant::now);

/// Provides the time that an assertion which waits, retries or measures
/// reads.
///
/// An assertion reads the clock of its seat, so a test that gives its seat a
/// [`Controlled`] clock decides how much time passes, and a busy machine does
/// not make the assertion flaky.
pub trait Clock: Debug + Send + Sync {
    /// Returns how long the clock has run since its origin.
    ///
    /// A clock states a duration rather than an instant, because a clock that
    /// a test controls has no instant of the wall to report.
    fn now(&self) -> Duration;

    /// Blocks until `duration` has passed on the clock.
    fn sleep(&self, duration: Duration);

    /// Moves a clock that a test controls forward by `duration`.
    ///
    /// The default does nothing, because the platform clock cannot move. An
    /// assertion that waits advances the clock of its seat and sleeps only
    /// when the advance did not move it.
    fn advance(&self, duration: Duration) {
        let _ = duration;
    }
}

/// Reads the platform clock.
///
/// This is the clock of every seat that supplies none.
#[derive(Debug, Clone, Copy)]
pub struct System {
    /// The instant this clock started at, or `None` for the clock that every
    /// seat shares, which starts at the process's first reading.
    started: Option<Instant>,
}

impl Default for System {
    fn default() -> Self {
        Self::new()
    }
}

impl System {
    /// Returns a clock that reads the platform from now.
    #[must_use]
    pub fn new() -> Self {
        Self {
            started: Some(Instant::now()),
        }
    }
}

/// The platform clock of every seat that supplies none, which starts at the
/// process's first reading.
pub(crate) static SHARED: System = System { started: None };

impl Clock for System {
    fn now(&self) -> Duration {
        self.started.unwrap_or(*ORIGIN).elapsed()
    }

    fn sleep(&self, duration: Duration) {
        std::thread::sleep(duration);
    }
}

/// A clock that moves only when a test advances it.
///
/// [`Clock::now`] returns the duration that the advances added up to.
/// [`Clock::sleep`] blocks until another thread advances the clock past the
/// duration. An assertion that retries advances the clock between its
/// attempts, so a body that passes on its third attempt costs three attempts
/// and no waiting.
///
/// Code under test that reads the platform clock directly reads another time,
/// and the controlled clock cannot detect it.
///
/// ```
/// use dokimi_assert::{Clock, Controlled};
/// use std::time::Duration;
///
/// let clock = Controlled::new();
/// clock.advance(Duration::from_secs(3));
/// assert_eq!(clock.now(), Duration::from_secs(3));
/// ```
#[derive(Debug, Default)]
pub struct Controlled {
    now: Mutex<Duration>,
    moved: Condvar,
}

impl Controlled {
    /// Returns a clock at zero.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Takes the clock's lock. A thread that panicked while it held the lock
    /// left a valid duration, so the lock's poison is ignored.
    fn locked(&self) -> MutexGuard<'_, Duration> {
        self.now.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Clock for Controlled {
    fn now(&self) -> Duration {
        *self.locked()
    }

    /// Blocks until another thread advances the clock past `duration` from
    /// the moment of the call. A `duration` of zero returns at once.
    fn sleep(&self, duration: Duration) {
        let mut now = self.locked();
        let until = *now + duration;
        while *now < until {
            now = self.moved.wait(now).unwrap_or_else(PoisonError::into_inner);
        }
    }

    fn advance(&self, duration: Duration) {
        *self.locked() += duration;
        self.moved.notify_all();
    }
}
