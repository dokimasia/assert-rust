//! How neighbouring items relate.

use dokimi_assert::seat::Recorder;
use dokimi_assert::{check, soft};
use std::sync::Mutex;

#[test]
fn a_sorted_sequence_passes_and_an_unsorted_one_is_reported() {
    let seat = Recorder::new();
    check::pairwise(&seat, &[1, 2, 2, 5], |a, b| a <= b, "the page is sorted");
    assert!(!seat.failed(), "{}", seat.message());

    let failing = Recorder::new();
    check::pairwise(&failing, &[1, 5, 2], |a, b| a <= b, "the page is sorted");
    assert!(failing.failed(), "an out-of-order pair must be reported");
}

#[test]
fn the_failure_names_the_index() {
    let seat = Recorder::new();
    check::pairwise(&seat, &[1, 2, 9, 3], |a, b| a <= b, "the page is sorted");

    assert!(seat.message().contains("index 2"), "{}", seat.message());
    assert!(seat.message().contains('9'), "{}", seat.message());
}

#[test]
fn nought_and_one_item_pass_having_no_pair() {
    let empty = Recorder::new();
    check::pairwise(&empty, &[] as &[i32], |a, b| a <= b, "the page is sorted");
    assert!(!empty.failed(), "{}", empty.message());

    let single = Recorder::new();
    check::pairwise(&single, &[7], |a, b| a <= b, "the page is sorted");
    assert!(!single.failed(), "{}", single.message());
}

#[test]
fn only_the_first_failing_pair_is_reported() {
    let seat = Recorder::new();
    soft::pairwise(&seat, &[9, 1, 8, 2], |a, b| a <= b, "the page is sorted");

    assert_eq!(
        seat.messages().len(),
        1,
        "one failure, not one per broken pair"
    );
    assert!(seat.message().contains("index 0"), "{}", seat.message());
}

#[test]
fn the_predicate_stops_at_the_first_failing_pair() {
    let seen = Mutex::new(Vec::new());
    let seat = Recorder::new();

    check::pairwise(
        &seat,
        &[1, 9, 2, 3],
        |a, b| {
            seen.lock().expect("the lock is held briefly").push(*a);
            a <= b
        },
        "the page is sorted",
    );

    assert_eq!(
        *seen.lock().expect("the lock is held briefly"),
        vec![1, 9],
        "walking past a known failure only wastes time"
    );
}

#[test]
fn uniqueness_and_strict_increase_are_the_same_assertion() {
    let unique = Recorder::new();
    check::pairwise(
        &unique,
        &["a", "b", "c"],
        |a, b| a != b,
        "no run of duplicates",
    );
    assert!(!unique.failed(), "{}", unique.message());

    let strict = Recorder::new();
    check::pairwise(&strict, &[1, 2, 2], |a, b| a < b, "strictly increasing");
    assert!(strict.failed(), "a repeat is not a strict increase");
}
