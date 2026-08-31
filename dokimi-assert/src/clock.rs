//! Where an assertion reads time.
//!
//! An assertion that waits, retries or measures reads a clock the seat carries rather
//! than calling the platform, so a test can supply time it controls and a busy machine
//! cannot make the assertion flaky.

use std::sync::{Condvar, LazyLock, Mutex};
use std::time::{Duration, Instant};

/// When the process started reading time, for the shared platform clock.
static ORIGIN: LazyLock<Instant> = LazyLock::new(Instant::now);

/// The two readings an assertion needs from time.
pub trait Clock: std::fmt::Debug + Send + Sync {
    /// Answer how long this clock has been running.
    ///
    /// A duration from the clock's own origin rather than an instant, because a clock a
    /// test controls has no wall-clock origin to report.
    fn now(&self) -> Duration;

    /// Block until the duration has passed on this clock.
    fn sleep(&self, duration: Duration);

    /// Move the clock forward, for a clock a test controls.
    ///
    /// The platform clock cannot be moved and ignores this, which is what lets an
    /// assertion advance whatever clock it was handed rather than ask which kind it is.
    fn advance(&self, _duration: Duration) {}
}

/// Reads the platform clock.
///
/// This is what an assertion gets when the seat carries no other, so an assertion that
/// reads time behaves as it did before a clock existed.
#[derive(Debug)]
pub struct System {
    /// When this clock started, or `None` for the shared one, which reads from the
    /// process rather than from a moment a caller chose. A static cannot call
    /// `Instant::now`, so the shared clock takes its origin on first read.
    started: Option<Instant>,
}

impl Default for System {
    fn default() -> Self {
        Self::new()
    }
}

impl System {
    /// Return a clock reading the platform, from now.
    #[must_use]
    pub fn new() -> Self {
        Self {
            started: Some(Instant::now()),
        }
    }

    /// The clock a seat gets when it supplies none.
    ///
    /// Readable in a `static`, which `new` is not: a static cannot call
    /// `Instant::now`.
    #[must_use]
    pub const fn shared() -> Self {
        Self { started: None }
    }
}

impl Clock for System {
    fn now(&self) -> Duration {
        self.started
            .map_or_else(|| ORIGIN.elapsed(), |at| at.elapsed())
    }

    fn sleep(&self, duration: Duration) {
        std::thread::sleep(duration);
    }
}

/// A clock that moves only when a test advances it.
///
/// [`Clock::now`] answers what [`Clock::advance`] last left it at, and [`Clock::sleep`]
/// blocks until the clock has passed the duration rather than until the wall has. An
/// assertion that retries advances this clock between attempts rather than sleeping
/// against it, so a body that settles on the third attempt costs three attempts and no
/// waiting.
///
/// A controlled clock cannot reach the subject: code under test that calls the platform
/// directly reads a different now, and nothing here detects that.
#[derive(Debug, Default)]
pub struct Controlled {
    instant: Mutex<Duration>,
    woke: Condvar,
}

impl Controlled {
    /// Return a clock reading zero until it is advanced.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl Clock for Controlled {
    /// # Panics
    ///
    /// If another thread panicked while holding this clock's lock.
    fn now(&self) -> Duration {
        *self
            .instant
            .lock()
            .expect("the clock's lock is not poisoned")
    }

    /// Block until the clock has passed the duration.
    ///
    /// It returns at once when the duration is zero. Otherwise it waits for another
    /// thread to advance the clock, so a test that sleeps on the only thread it has
    /// blocks until something advances it.
    ///
    /// The duration is measured from the instant this reads, so a caller racing sleep
    /// against advance on two threads cannot say which instant it slept from. Assertions
    /// do not hit this: one that retries advances the clock itself, on the thread it is
    /// already running on.
    ///
    /// # Panics
    ///
    /// If another thread panicked while holding this clock's lock.
    fn sleep(&self, duration: Duration) {
        if duration.is_zero() {
            return;
        }
        let mut held = self
            .instant
            .lock()
            .expect("the clock's lock is not poisoned");
        let until = *held + duration;
        while *held < until {
            held = self
                .woke
                .wait(held)
                .expect("the clock's lock is not poisoned");
        }
    }

    /// # Panics
    ///
    /// If another thread panicked while holding this clock's lock.
    fn advance(&self, duration: Duration) {
        if duration.is_zero() {
            return;
        }
        let mut held = self
            .instant
            .lock()
            .expect("the clock's lock is not poisoned");
        *held += duration;
        drop(held);
        self.woke.notify_all();
    }
}
