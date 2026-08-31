//! The behaviours a corpus case can name in place of a callable.
//!
//! A case states its arguments as typed literals, which cannot describe a callable.
//! The assertions taking one are handed a named behaviour from a small fixed set
//! instead, and this builds each one natively.

use dokimi_assert::matcher::behaviour::{Cancel, Stop};
use dokimi_assert::seat::{Recorder, Seat};
use dokimi_assert::{check, soft};
use std::cell::Cell;
use std::time::Duration;

/// How long a retrying assertion is given, against a controlled clock.
const RETRY_TIMEOUT: Duration = Duration::from_secs(3600);

/// How long it waits between attempts on that clock.
const RETRY_INTERVAL: Duration = Duration::from_secs(60);

/// What a subject answers when it stops, or fails on its own terms.
///
/// A subject honouring a handle has to say which reason it stopped for, and the
/// assertion reads that through the chain of causes. One that fails for its own
/// reason carries no cause, which is what tells the two apart.
#[derive(Debug)]
pub struct SubjectError {
    said: &'static str,
    because: Option<Stop>,
}

impl std::fmt::Display for SubjectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.said)
    }
}

impl std::error::Error for SubjectError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.because
            .as_ref()
            .map(|held| held as &(dyn std::error::Error + 'static))
    }
}

/// Which behaviour a case named, resolved once so the drivers do not each
/// re-read the string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Does the work and answers success.
    ReturnsOk,
    /// Answers whatever the handle says.
    ReadsHandle,
    /// Panics rather than answering.
    Raises,
    /// Answers a failure of its own.
    FailsOtherwise,
    /// Reads a handle without checking it is there.
    DereferencesHandle,
    /// Reports a failure on every attempt.
    NeverSettles,
    /// Reports a failure twice, then succeeds.
    SettlesAfter,
    /// Changes the observed state once per call.
    Accumulates,
    /// Reads the observed state and changes nothing.
    LeavesStateAlone,
}

impl Kind {
    /// Read the kind a case names, or `None` when this language builds no such
    /// behaviour.
    #[must_use]
    pub fn read(named: &str) -> Option<Self> {
        Some(match named {
            "returns-ok" | "ignores-handle" => Self::ReturnsOk,
            "reads-handle" => Self::ReadsHandle,
            "raises" => Self::Raises,
            "fails-otherwise" => Self::FailsOtherwise,
            "dereferences-handle" => Self::DereferencesHandle,
            "never-settles" => Self::NeverSettles,
            "settles-after" => Self::SettlesAfter,
            "accumulates" => Self::Accumulates,
            "leaves-state-alone" => Self::LeavesStateAlone,
            _ => return None,
        })
    }
}

/// Run the behaviour in the shape the cancellation assertions take.
fn handled(kind: Kind, handle: Option<&Cancel>) -> Result<(), SubjectError> {
    match kind {
        Kind::ReadsHandle => match handle.and_then(Cancel::stopped) {
            Some(stopped) => Err(SubjectError {
                said: "the subject gave up when told",
                because: Some(stopped),
            }),
            None => Ok(()),
        },
        Kind::Raises => panic!("the subject raised"),
        Kind::FailsOtherwise => Err(SubjectError {
            said: "the subject failed for its own reason",
            because: None,
        }),
        // Reading an absent handle is the behaviour under test: the assertion
        // asks whether a subject survives one.
        Kind::DereferencesHandle => {
            let _ = handle
                .expect("the subject reads its handle without checking")
                .stopped();
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Run the behaviour in the shape a retrying assertion takes.
fn seated(kind: Kind, attempts: &Cell<usize>, trial: &dyn Seat) {
    match kind {
        Kind::NeverSettles => trial.record("never settles"),
        Kind::SettlesAfter => {
            attempts.set(attempts.get() + 1);
            if attempts.get() < 3 {
                trial.record("not yet");
            }
        }
        _ => {}
    }
}

/// Drive one subject case on the named surface, answering whether it ran.
///
/// The assertions taking a callable differ in shape, so each says how it is
/// called here rather than the corpus runner knowing all of them.
///
/// # Panics
///
/// If a subject that is meant to panic is driven by an assertion that does not
/// catch one, which no case states.
pub fn run(surface: &str, assertion: &str, kind: Kind, seat: &Recorder, msg: &str) -> bool {
    let held = Cell::new(vec![1_usize, 2]);
    let attempts = Cell::new(0_usize);
    let changes = kind == Kind::Accumulates;

    let observe = || {
        let read = held.take();
        held.set(read.clone());
        read
    };
    let bare = || {
        if changes {
            let mut read = held.take();
            read.push(read.len());
            held.set(read);
        }
        assert!(kind != Kind::Raises, "the subject raised");
    };

    match (surface, assertion) {
        ("check", "throws") => {
            check::panics(seat, bare, msg);
        }
        ("check", "not-throws") => check::does_not_panic(seat, bare, msg),
        ("check", "honours-cancellation") => {
            check::honours_cancellation(seat, |h| handled(kind, h), msg);
        }
        ("check", "honours-deadline") => {
            check::honours_deadline(seat, |h| handled(kind, h), msg);
        }
        ("check", "nil-context-safe") => {
            check::none_handle_safe(seat, |h| handled(kind, h), msg);
        }
        ("check", "pure") => check::is_pure(seat, observe, bare, msg),
        ("check", "eventually") => check::eventually(
            seat,
            RETRY_TIMEOUT,
            RETRY_INTERVAL,
            |trial| seated(kind, &attempts, trial),
            msg,
        ),
        ("check", "eventually-true") => check::eventually_true(
            seat,
            RETRY_TIMEOUT,
            || {
                let trial = Recorder::new();
                seated(kind, &attempts, &trial);
                !trial.failed()
            },
            msg,
        ),
        ("soft", "throws") => {
            soft::panics(seat, bare, msg);
        }
        ("soft", "not-throws") => soft::does_not_panic(seat, bare, msg),
        ("soft", "honours-cancellation") => {
            soft::honours_cancellation(seat, |h| handled(kind, h), msg);
        }
        ("soft", "honours-deadline") => {
            soft::honours_deadline(seat, |h| handled(kind, h), msg);
        }
        ("soft", "nil-context-safe") => {
            soft::none_handle_safe(seat, |h| handled(kind, h), msg);
        }
        ("soft", "pure") => soft::is_pure(seat, observe, bare, msg),
        ("soft", "eventually") => soft::eventually(
            seat,
            RETRY_TIMEOUT,
            RETRY_INTERVAL,
            |trial| seated(kind, &attempts, trial),
            msg,
        ),
        ("soft", "eventually-true") => soft::eventually_true(
            seat,
            RETRY_TIMEOUT,
            || {
                let trial = Recorder::new();
                seated(kind, &attempts, &trial);
                !trial.failed()
            },
            msg,
        ),
        _ => return false,
    }
    true
}
