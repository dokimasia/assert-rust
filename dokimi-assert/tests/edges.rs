//! The text and number cases no corpus file can state.
//!
//! The corpus drives prefix, suffix, pattern, tolerance and range against values it can
//! encode. NaN is not a JSON number, and a pattern that does not compile is not a value at
//! all, so those live here.

use dokimi_assert::check;
use dokimi_assert::seat::Recorder;

#[test]
fn a_pattern_that_does_not_compile_is_reported_as_a_broken_pattern() {
    let seat = Recorder::new();
    check::matches(
        &seat,
        "GET /users",
        "[unclosed",
        "the line names the collection",
    );

    assert!(seat.failed(), "a pattern that cannot compile has to say so");
    assert!(named(&seat, "matches"), "{}", seat.message());
    // Without this the message reads as a subject that failed to match,
    // and the typo sends someone looking for a bug in the code under test.
    assert!(
        seat.failures()[0]
            .detail("pattern")
            .is_some_and(|held| held.to_string().contains("[unclosed")),
        "{}",
        seat.message()
    );
}

#[test]
fn matches_searches_rather_than_anchoring() {
    let searching = Recorder::new();
    check::matches(
        &searching,
        "GET /users 200",
        "users",
        "the line names the collection",
    );
    assert!(!searching.failed(), "{}", searching.message());

    let anchored = Recorder::new();
    check::matches(
        &anchored,
        "GET /users 200",
        "^users",
        "the line starts with the path",
    );
    assert!(
        anchored.failed(),
        "an anchor is honoured where one is written"
    );
}

#[test]
fn nan_is_outside_every_tolerance() {
    for (got, want, tolerance, why) in [
        (f64::NAN, 1.0, 0.1, "a NaN reading is not close to anything"),
        (1.0, f64::NAN, 0.1, "nothing is close to NaN"),
        (
            1.0,
            1.0,
            f64::NAN,
            "a NaN tolerance admits nothing, not everything",
        ),
        (
            f64::NAN,
            1.0,
            f64::INFINITY,
            "widening the tolerance does not make NaN a number",
        ),
    ] {
        let seat = Recorder::new();
        check::close_to(&seat, got, want, tolerance, "the rate is about one");
        assert!(seat.failed(), "{why}");
        assert!(named(&seat, "close-to"), "{}", seat.message());
    }
}

#[test]
fn the_tolerance_bound_is_inclusive() {
    let seat = Recorder::new();
    check::close_to(&seat, 1.5, 1.0, 0.5, "the rate is about one");
    assert!(
        !seat.failed(),
        "a difference exactly equal to the tolerance passes"
    );
}

#[test]
fn nan_is_in_no_range() {
    let seat = Recorder::new();
    check::in_range(
        &seat,
        f64::NAN,
        f64::NEG_INFINITY,
        f64::INFINITY,
        "the reading is a number",
    );
    assert!(
        seat.failed(),
        "the widest range there is still does not hold NaN"
    );
}

#[test]
fn a_range_with_the_bounds_reversed_says_so() {
    let seat = Recorder::new();
    check::in_range(&seat, 5.0, 10.0, 1.0, "the page size is sane");

    assert!(
        seat.failed(),
        "a range that can hold nothing is the mistake, not the value"
    );
    assert!(named(&seat, "in-range"), "{}", seat.message());
}

#[test]
fn both_range_bounds_are_inside_it() {
    for got in [1.0, 10.0] {
        let seat = Recorder::new();
        check::in_range(&seat, got, 1.0, 10.0, "the page size is sane");
        assert!(
            !seat.failed(),
            "{got} is a bound and bounds are inside: {}",
            seat.message()
        );
    }
}

#[test]
fn length_counts_characters_rather_than_bytes() {
    // A value that reads as one character counts as one however it encodes.
    let seat = Recorder::new();
    check::length(&seat, "é", 1, "the accented letter is one character");
    assert!(!seat.failed(), "{}", seat.message());
}

#[test]
fn equality_is_the_languages_own_and_already_says_what_the_standard_wants() {
    let nan = Recorder::new();
    check::equal(&nan, &f64::NAN, &f64::NAN, "NaN is unequal to itself");
    assert!(
        nan.failed(),
        "IEEE 754 says NaN does not equal itself, and so does Rust"
    );

    let zeroes = Recorder::new();
    check::equal(&zeroes, &0.0_f64, &-0.0_f64, "the two zeroes are equal");
    assert!(!zeroes.failed(), "{}", zeroes.message());

    let arrays = Recorder::new();
    check::equal(
        &arrays,
        &vec![1, 2],
        &vec![1, 2],
        "containers compare by their elements",
    );
    assert!(!arrays.failed(), "{}", arrays.message());
}

/// Whether the seat's first record names that assertion.
fn named(seat: &Recorder, assertion: &str) -> bool {
    seat.failures()
        .first()
        .is_some_and(|held| held.assertion == assertion)
}
