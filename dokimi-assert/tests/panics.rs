//! A call that panics, and one that does not.

use dokimi_assert::check;
use dokimi_assert::seat::Recorder;

#[test]
fn panics_yields_what_was_raised() {
    let seat = Recorder::new();
    let raised = check::panics(
        &seat,
        || panic!("store is closed"),
        "saving refuses a closed store",
    );

    assert!(!seat.failed(), "{}", seat.message());
    assert_eq!(
        raised.as_deref(),
        Some("store is closed"),
        "the message is handed back"
    );
}

#[test]
fn panics_reports_a_body_that_returns() {
    let seat = Recorder::new();
    let raised = check::panics(&seat, || {}, "saving refuses");

    assert!(seat.failed(), "a body that returns must be reported");
    assert!(
        raised.is_none(),
        "nothing panicked, so there is nothing to hand back"
    );
    assert!(named(&seat, "throws"), "{}", seat.message());
}

#[test]
fn panics_reads_a_formatted_message() {
    let seat = Recorder::new();
    let key = "widget";
    let raised = check::panics(&seat, || panic!("no such key: {key}"), "the key is checked");

    assert_eq!(
        raised.as_deref(),
        Some("no such key: widget"),
        "a String payload reads too"
    );
}

#[test]
fn does_not_panic_passes_a_quiet_body_and_reports_a_panicking_one() {
    let passing = Recorder::new();
    check::does_not_panic(&passing, || {}, "parsing accepts this input");
    assert!(!passing.failed(), "{}", passing.message());

    let failing = Recorder::new();
    check::does_not_panic(
        &failing,
        || panic!("bad input"),
        "parsing accepts this input",
    );
    assert!(failing.failed(), "a panic must be reported");
    assert!(
        failing.failures()[0]
            .detail("got")
            .is_some_and(|got| got.to_string().contains("bad input")),
        "{}",
        failing.message()
    );
}

#[test]
fn an_expected_panic_leaves_the_hook_working_for_other_threads() {
    // The hook is process-wide and the harness runs tests on parallel
    // threads, so silencing one thread's panic must not silence another's.
    let seat = Recorder::new();
    check::panics(&seat, || panic!("expected"), "this one is asked for");

    let elsewhere = std::thread::spawn(|| std::panic::catch_unwind(|| panic!("unrelated")));
    assert!(
        elsewhere.join().expect("the thread runs").is_err(),
        "an unrelated panic still unwinds"
    );
}

/// Whether the seat's first record names that assertion.
fn named(seat: &Recorder, assertion: &str) -> bool {
    seat.failures()
        .first()
        .is_some_and(|held| held.assertion == assertion)
}
