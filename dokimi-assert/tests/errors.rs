//! Failures handed back as values, and the chain of sources.
//!
//! Every assertion is driven twice, with an error that satisfies it and one that does
//! not. An assertion that reports nothing whatever it is handed passes a one-sided test.

use dokimi_assert::seat::Recorder;
use dokimi_assert::{check, soft};
use std::error::Error;
use std::fmt;

/// An error with a name of its own, so a downcast is not a match on anything.
#[derive(Debug, PartialEq)]
struct Refused(&'static str);

impl fmt::Display for Refused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "refused: {}", self.0)
    }
}

impl Error for Refused {}

/// An error wrapping another, so a test can prove the chain is walked.
#[derive(Debug)]
struct WhileSaving(Refused);

impl fmt::Display for WhileSaving {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "while saving")
    }
}

impl Error for WhileSaving {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.0)
    }
}

#[test]
fn no_error_passes_ok_and_reports_err() {
    let seat = Recorder::new();
    check::no_error(&seat, &Ok::<(), Refused>(()), "the call succeeds");
    assert!(!seat.failed(), "{}", seat.message());

    let failing = Recorder::new();
    check::no_error(
        &failing,
        &Err::<(), _>(Refused("no room")),
        "the call succeeds",
    );
    assert!(failing.failed(), "an error must be reported");
    assert!(
        failing.message().contains("no room"),
        "{}",
        failing.message()
    );
}

#[test]
fn has_error_passes_err_and_reports_ok() {
    let seat = Recorder::new();
    check::has_error(&seat, &Err::<(), _>(Refused("no room")), "the call refuses");
    assert!(!seat.failed(), "{}", seat.message());

    let failing = Recorder::new();
    check::has_error(&failing, &Ok::<i32, Refused>(7), "the call refuses");
    assert!(failing.failed(), "the absence of an error must be reported");
    assert!(named(&failing, "err-present"), "{}", failing.message());
}

#[test]
fn error_is_matches_and_rejects() {
    let seat = Recorder::new();
    check::error_is(
        &seat,
        &Refused("no room"),
        &Refused("no room"),
        "the reason is stated",
    );
    assert!(!seat.failed(), "{}", seat.message());

    let failing = Recorder::new();
    check::error_is(
        &failing,
        &Refused("some other reason"),
        &Refused("no room"),
        "the reason is stated",
    );
    assert!(failing.failed(), "a different value must be reported");
    assert!(named(&failing, "err-is"), "{}", failing.message());
}

#[test]
fn error_is_reaches_a_wrapped_source() {
    let seat = Recorder::new();
    check::error_is(
        &seat,
        &WhileSaving(Refused("no room")),
        &Refused("no room"),
        "the reason survives wrapping",
    );
    assert!(!seat.failed(), "{}", seat.message());
}

#[test]
fn error_is_not_is_the_mirror() {
    let seat = Recorder::new();
    check::error_is_not(
        &seat,
        &Refused("no room"),
        &Refused("other"),
        "it fails otherwise",
    );
    assert!(!seat.failed(), "{}", seat.message());

    let failing = Recorder::new();
    check::error_is_not(
        &failing,
        &Refused("no room"),
        &Refused("no room"),
        "it fails otherwise",
    );
    assert!(failing.failed(), "a match must be reported");
    assert!(named(&failing, "err-is-not"), "{}", failing.message());
}

#[test]
fn error_as_yields_the_typed_error() {
    let wrapped = WhileSaving(Refused("no room"));

    let seat = Recorder::new();
    let found: Option<&Refused> = check::error_as(&seat, &wrapped, "the reason is typed");
    assert!(!seat.failed(), "{}", seat.message());
    assert_eq!(
        found,
        Some(&Refused("no room")),
        "the borrow is what carries the fields"
    );
}

#[test]
fn error_as_answers_none_and_reports_when_nothing_matches() {
    let seat = Recorder::new();
    let found: Option<&WhileSaving> =
        check::error_as(&seat, &Refused("no room"), "the reason is typed");

    assert!(
        found.is_none(),
        "nothing matched, so there is nothing to hand back"
    );
    assert!(seat.failed(), "a chain with no match must be reported");
    assert!(seat.message().contains("WhileSaving"), "{}", seat.message());
}

#[test]
fn a_chain_that_loops_stops_rather_than_spinning() {
    // A source that answers itself is a cycle. Walking it has to end.
    #[derive(Debug)]
    struct Loops;
    impl fmt::Display for Loops {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "loops")
        }
    }
    impl Error for Loops {
        fn source(&self) -> Option<&(dyn Error + 'static)> {
            Some(&Loops)
        }
    }

    let seat = Recorder::new();
    check::error_is(
        &seat,
        &Loops,
        &Refused("no room"),
        "the chain is walked once",
    );
    assert!(
        seat.failed(),
        "nothing in the loop matches, so it must be reported"
    );
}

#[test]
fn both_surfaces_report_the_same_text() {
    let aborting = Recorder::new();
    check::no_error(
        &aborting,
        &Err::<(), _>(Refused("no room")),
        "the call succeeds",
    );

    let recording = Recorder::new();
    soft::no_error(
        &recording,
        &Err::<(), _>(Refused("no room")),
        "the call succeeds",
    );

    assert_eq!(
        aborting.message(),
        recording.message(),
        "only the path differs"
    );
    assert!(
        aborting.messages().is_empty(),
        "check uses the aborting path"
    );
    assert_eq!(
        recording.messages().len(),
        1,
        "soft uses the recording path"
    );
}

/// Whether the seat's first record names that assertion.
fn named(seat: &Recorder, assertion: &str) -> bool {
    seat.failures()
        .first()
        .is_some_and(|held| held.assertion == assertion)
}
