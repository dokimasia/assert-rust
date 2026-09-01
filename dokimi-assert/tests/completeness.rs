//! Every assertion the standard names exists, under the name the naming table gives it.
//!
//! The other implementations of this standard ask their runtime whether a member exists:
//! Python reaches for `getattr`, Java for reflection, TypeScript for the keys of a module.
//! Rust can ask nothing at run time, so the gate is a compile-time one instead, which is
//! the stronger of the two: every name below is referenced as a value of its own type, so
//! renaming an assertion or changing its shape fails the build rather than a test.
//!
//! What is checked at run time is only that this file has not fallen behind the table.

use dokimi_assert::clock::{Clock, Controlled, System};
use dokimi_assert::failure::{Detail, Failure, Where};
use dokimi_assert::seat::{Recorder, Seat};
use dokimi_assert::{bench, check, golden, soft};
use std::error::Error;
use std::fmt;
use std::path::Path;
use std::time::Duration;

/// A stand-in error, since several assertions are generic over one.
#[derive(Debug, PartialEq)]
struct Sample;

impl fmt::Display for Sample {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "sample")
    }
}

impl Error for Sample {}

/// Name every assertion, as a value of its own type.
///
/// Nothing here is called. Naming a function as a value is what makes the compiler check
/// it exists with that signature, which is the whole point: a test that called them would
/// be testing behaviour, and behaviour is the corpus's job.
#[test]
#[expect(
    clippy::type_complexity,
    reason = "these are the signatures being pinned"
)]
fn every_assertion_exists_under_its_name() {
    type Cancel = dokimi_assert::matcher::behaviour::Cancel;
    type Body = fn(Option<&Cancel>) -> Result<(), Sample>;

    // values
    let _: fn(&dyn Seat, &i32, &i32, &str) = check::equal::<i32>;
    let _: fn(&dyn Seat, &i32, &i32, &str) = check::not_equal::<i32>;
    let _: fn(&dyn Seat, bool, &str) = check::is_true;
    let _: fn(&dyn Seat, bool, &str) = check::is_false;
    let _: fn(&dyn Seat, Option<&i32>, &str) = check::is_none::<i32>;
    let _: fn(&dyn Seat, Option<&i32>, &str) = check::is_some::<i32>;

    // sizes
    let _: fn(&dyn Seat, &str, usize, &str) = check::length::<str>;
    let _: fn(&dyn Seat, &str, &str) = check::is_empty::<str>;
    let _: fn(&dyn Seat, &str, &str) = check::is_not_empty::<str>;

    // containment
    let _: fn(&dyn Seat, &str, &str, &str) = check::contains::<str, str>;
    let _: fn(&dyn Seat, &str, &str, &str) = check::not_contains::<str, str>;
    let _: fn(&dyn Seat, &str, &[&str], &str) = check::contains_in_order;

    // text
    let _: fn(&dyn Seat, &str, &str, &str) = check::has_prefix;
    let _: fn(&dyn Seat, &str, &str, &str) = check::has_suffix;
    let _: fn(&dyn Seat, &str, &str, &str) = check::matches;

    // numbers
    let _: fn(&dyn Seat, f64, f64, f64, &str) = check::close_to;
    let _: fn(&dyn Seat, f64, f64, f64, &str) = check::in_range;

    // errors
    let _: fn(&dyn Seat, &Result<(), Sample>, &str) = check::no_error::<(), Sample>;
    let _: fn(&dyn Seat, &Result<(), Sample>, &str) = check::has_error::<(), Sample>;
    let _: fn(&dyn Seat, &(dyn Error + 'static), &Sample, &str) = check::error_is::<Sample>;
    let _: fn(&dyn Seat, &(dyn Error + 'static), &Sample, &str) = check::error_is_not::<Sample>;
    pins_error_as();

    // panicking
    let _: fn(&dyn Seat, fn(), &str) -> Option<String> = check::panics::<fn()>;
    let _: fn(&dyn Seat, fn(), &str) = check::does_not_panic::<fn()>;

    // ordering
    let _: fn(&dyn Seat, &[i32], fn(&i32, &i32) -> bool, &str) =
        check::pairwise::<i32, fn(&i32, &i32) -> bool>;

    // behaviour
    let _: fn(&dyn Seat, Body, &str) = check::honours_cancellation::<Sample, Body>;
    let _: fn(&dyn Seat, Body, &str) = check::honours_deadline::<Sample, Body>;
    let _: fn(&dyn Seat, Duration, Body, &str) = check::completes_within::<Sample, Body>;
    let _: fn(&dyn Seat, fn() -> i32, fn(), &str) = check::is_pure::<i32, fn() -> i32, fn()>;
    let _: fn(&dyn Seat, Body, &str) = check::none_handle_safe::<Sample, Body>;

    // waiting
    let _: fn(&dyn Seat, Duration, Duration, fn(&Recorder), &str) =
        check::eventually::<fn(&Recorder)>;
    let _: fn(&dyn Seat, Duration, fn() -> bool, &str) = check::eventually_true::<fn() -> bool>;

    // testing an assertion, on the aborting surface only
    let _: fn(&dyn Seat, fn(&Recorder), &str) -> String = check::rejects::<fn(&Recorder)>;

    // golden files
    let _: fn(&dyn Seat, &str, &str, &[golden::Scrubber]) = golden::matches;
    let _: fn(&dyn Seat, &Path, &str, &[golden::Scrubber]) = golden::matches_at;
    let _: fn(&dyn Seat, &Path, &str, &str, &[golden::Scrubber]) = golden::matches_json_field;

    // benchmark ceilings
    pins_bench();
}

/// Pin the assertions whose type mentions a lifetime.
///
/// A `fn` pointer written with `'_` is higher-ranked over every lifetime, which these
/// are not: each borrows for one. Naming the lifetime is what lets the annotation be
/// written at all.
#[expect(
    clippy::extra_unused_lifetimes,
    reason = "the lifetime is what lets the annotation inside be written at all"
)]
fn pins_error_as<'e>() {
    let _: fn(&dyn Seat, &'e (dyn Error + 'static), &str) -> Option<&'e Sample> =
        check::error_as::<Sample>;
}

#[expect(
    clippy::extra_unused_lifetimes,
    reason = "the lifetime is what lets the annotation inside be written at all"
)]
fn pins_bench<'s>() {
    let _: fn(bench::Contract<'s>, u64) -> bench::Contract<'s> = bench::Contract::max_allocs;
    let _: fn(bench::Contract<'s>, u64) -> bench::Contract<'s> = bench::Contract::max_bytes;
    let _: fn(bench::Contract<'s>, Duration) -> bench::Contract<'s> = bench::Contract::max_latency;
    let _: fn(bench::Contract<'s>, Duration) -> bench::Contract<'s> = bench::Contract::max_mean;
}

/// The recording surface carries every member of the aborting one but `rejects`.
#[test]
fn both_surfaces_carry_the_same_assertions() {
    let _: fn(&dyn Seat, &i32, &i32, &str) = soft::equal::<i32>;
    let _: fn(&dyn Seat, &i32, &i32, &str) = soft::not_equal::<i32>;
    let _: fn(&dyn Seat, bool, &str) = soft::is_true;
    let _: fn(&dyn Seat, bool, &str) = soft::is_false;
    let _: fn(&dyn Seat, Option<&i32>, &str) = soft::is_none::<i32>;
    let _: fn(&dyn Seat, Option<&i32>, &str) = soft::is_some::<i32>;
    let _: fn(&dyn Seat, &str, usize, &str) = soft::length::<str>;
    let _: fn(&dyn Seat, &str, &str) = soft::is_empty::<str>;
    let _: fn(&dyn Seat, &str, &str) = soft::is_not_empty::<str>;
    let _: fn(&dyn Seat, &str, &str, &str) = soft::contains::<str, str>;
    let _: fn(&dyn Seat, &str, &str, &str) = soft::not_contains::<str, str>;
    let _: fn(&dyn Seat, &str, &[&str], &str) = soft::contains_in_order;
    let _: fn(&dyn Seat, &str, &str, &str) = soft::has_prefix;
    let _: fn(&dyn Seat, &str, &str, &str) = soft::has_suffix;
    let _: fn(&dyn Seat, &str, &str, &str) = soft::matches;
    let _: fn(&dyn Seat, f64, f64, f64, &str) = soft::close_to;
    let _: fn(&dyn Seat, f64, f64, f64, &str) = soft::in_range;
    let _: fn(&dyn Seat, &Result<(), Sample>, &str) = soft::no_error::<(), Sample>;
    let _: fn(&dyn Seat, &Result<(), Sample>, &str) = soft::has_error::<(), Sample>;
    let _: fn(&dyn Seat, &(dyn Error + 'static), &Sample, &str) = soft::error_is::<Sample>;
    let _: fn(&dyn Seat, &(dyn Error + 'static), &Sample, &str) = soft::error_is_not::<Sample>;
    let _: fn(&dyn Seat, fn(), &str) = soft::does_not_panic::<fn()>;
}

/// Every name in the table is one this file pins.
///
/// The list above is written by hand, so this is what stops it falling behind the
/// standard: a new assertion appears in the table and fails here until someone names it.
#[test]
fn the_table_names_nothing_this_file_leaves_out() {
    let raw = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/spec/naming.json"),
    )
    .expect("the vendored naming table is readable");
    let table: serde_json::Value = serde_json::from_str(&raw).expect("the naming table is JSON");

    let named: Vec<String> = table["names"]
        .as_object()
        .expect("the table names assertions")
        .iter()
        .map(|(id, langs)| {
            let name = langs["rust"]
                .as_str()
                .unwrap_or_else(|| panic!("{id} has no rust name"));
            format!("{id} -> {name}")
        })
        .collect();

    assert_eq!(named.len(), 41, "the standard states 41 assertions");

    // Read at compile time: file!() is relative to the workspace root
    // and a test runs from its own crate, so the two do not meet.
    let source = include_str!("completeness.rs");
    let mut missing: Vec<&str> = Vec::new();
    for entry in &named {
        // no_task_leaks ships in dokimi-assert-tokio, which pins it in
        // a gate of its own. Rust's standard library cannot enumerate
        // threads, so nothing in this crate could answer the question;
        // the overlay records that as a limit.
        if entry.starts_with("no-task-leaks ") {
            continue;
        }
        let name = entry
            .split(" -> ")
            .nth(1)
            .expect("an entry names something");
        // The leaf is what appears in the reference: a qualified name
        // reaches it through a module or a type.
        let leaf = name.rsplit("::").next().expect("a name has a last segment");
        if !source.contains(leaf) {
            missing.push(name);
        }
    }
    assert!(
        missing.is_empty(),
        "the table names assertions this file does not pin: {missing:?}"
    );
}

/// Every relaxation the definition states is answered, one way.
///
/// Rust offers neither: its types keep an absent container and an empty one
/// apart, and its own `==` already says NaN is unequal to itself. That is a
/// claim, so this holds it to the overlay rather than trusting the sentence.
/// A relaxation the naming table gave Rust a name for would have to exist,
/// and one it did not would have to be declined; named and declined at once
/// is a contradiction.
#[test]
fn every_relaxation_is_offered_or_declined() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/spec");
    let parse = |name: &str| -> serde_json::Value {
        serde_json::from_str(
            &std::fs::read_to_string(dir.join(name)).expect("the vendored spec is readable"),
        )
        .expect("the vendored spec is JSON")
    };

    let stated = parse("assertions.json");
    let stated = stated["relaxations"]
        .as_object()
        .expect("the definition states relaxations");
    assert!(!stated.is_empty(), "the definition states relaxations");

    let naming = parse("naming.json");
    let overlay = parse("overlay.json");
    let declined: Vec<&str> = overlay["relaxations"]
        .as_array()
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| entry["id"].as_str())
                .collect()
        })
        .unwrap_or_default();

    for id in stated.keys() {
        let named = naming["relaxations"][id]["rust"].as_str();
        let is_declined = declined.contains(&id.as_str());

        assert!(
            !(named.is_some() && is_declined),
            "{id}: named {named:?} and declined, which is a contradiction"
        );
        assert!(
            named.is_some() || is_declined,
            "{id}: the table gives no Rust name and the overlay does not decline it"
        );
        // A named relaxation would need a compile-time pin in this file, the
        // way every assertion has one. None is named today, so a name
        // appearing is this test failing until someone pins it.
        assert!(
            named.is_none(),
            "{id}: the table now names {named:?} for Rust; implement it and pin it here"
        );
    }
}

/// Name every surface row the table gives Rust, as a value of its own type.
///
/// The same rule as the assertions: nothing here is called, and a rename or a
/// changed shape fails the build rather than a test.
#[test]
fn every_surface_row_exists_under_its_name() {
    // types
    let _: Option<&dyn Seat> = None;
    let _: fn() -> dokimi_assert::seat::Standard = dokimi_assert::seat::Standard::new;
    let _: fn() -> Recorder = Recorder::new;
    let _: fn() -> dokimi_assert::seat::Collector = dokimi_assert::seat::Collector::new;
    let _: fn() -> golden::Scrubber = golden::scrub_hashes;
    pins_contract_type();

    // members: the seat's three, through a concrete implementation
    let _: fn(&Recorder) = <Recorder as Seat>::helper;
    let _: fn(&Recorder, &str) = <Recorder as Seat>::fail;
    let _: fn(&Recorder, &str) = <Recorder as Seat>::record;

    // members: what a test reads back
    let _: fn(&Recorder) -> bool = Recorder::failed;
    let _: fn(&Recorder) -> String = Recorder::message;
    let _: fn(&Recorder) -> Vec<String> = Recorder::messages;
    let _: fn(&Recorder) -> usize = Recorder::helper_calls;
    let _: fn(&dokimi_assert::seat::Collector) -> Vec<String> =
        dokimi_assert::seat::Collector::collected;
    let _: fn(&dokimi_assert::seat::Collector) = dokimi_assert::seat::Collector::flush;

    // the clock a seat carries, and the one a test moves
    let _: Option<&dyn Clock> = None;
    let _: fn() -> Controlled = Controlled::new;
    let _: fn() -> System = System::new;
    let _: Option<Failure> = None;
    let _: Option<Where> = None;
    pins_failure_detail();
    let _: fn(&Recorder, &Failure, bool) = <Recorder as Seat>::report;
    let _: fn(&Controlled) -> Duration = <Controlled as Clock>::now;
    let _: fn(&Controlled, Duration) = <Controlled as Clock>::sleep;
    let _: fn(&Controlled, Duration) = <Controlled as Clock>::advance;
    let _: fn(&Recorder) -> &dyn Clock = <Recorder as Seat>::clock;
    let _: fn(&Recorder) -> Vec<Failure> = Recorder::failures;

    // helpers
    let _: fn() -> golden::Scrubber = golden::scrub_timestamps;
    let _: fn() -> golden::Scrubber = golden::scrub_hashes;
    let _: fn() -> golden::Scrubber = golden::scrub_run_ids;
    let _: fn(&[&str]) -> golden::Scrubber = golden::scrub_json_fields;
    let _: fn() -> bool = golden::should_update;
}

/// The record's reader, pinned with the lifetime it borrows for.
#[expect(
    clippy::extra_unused_lifetimes,
    reason = "the lifetime is what lets the annotation inside be written at all"
)]
fn pins_failure_detail<'f>() {
    let _: fn(&'f Failure, &str) -> Option<&'f Detail> = Failure::detail;
}

/// The shape a contract's setup-taking runner has, named so the pin below
/// reads as one line rather than four.
type Measuring<'s> = fn(bench::Contract<'s>, usize, fn() -> u8, fn(u8)) -> bench::Contract<'s>;

/// The contract rows, pinned with the lifetime their type carries.
#[expect(
    clippy::extra_unused_lifetimes,
    reason = "the lifetime is what lets the annotation inside be written at all"
)]
fn pins_contract_type<'s>() {
    let _: fn(&'s dyn Seat, &'s str) -> bench::Contract<'s> = bench::Contract::new;
    let _: fn(bench::Contract<'s>, usize, fn()) -> bench::Contract<'s> =
        bench::Contract::run::<fn()>;
    let _: fn(bench::Contract<'s>) = bench::Contract::check;
    let _: Measuring<'s> = bench::Contract::measuring::<u8, fn() -> u8, fn(u8)>;
}

/// Every surface id the table names for Rust is pinned above.
#[test]
fn the_surface_table_names_nothing_this_file_leaves_out() {
    let raw = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/spec/naming.json"),
    )
    .expect("the vendored naming table is readable");
    let table: serde_json::Value = serde_json::from_str(&raw).expect("the naming table is JSON");

    let surface = table["surface"]
        .as_object()
        .expect("the table states a surface");

    // A row the overlay declines is answered, so it needs no pin.
    let overlay_raw = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/spec/overlay.json"),
    )
    .expect("the vendored overlay is readable");
    let overlay: serde_json::Value =
        serde_json::from_str(&overlay_raw).expect("the overlay is JSON");
    let waived: Vec<&str> = overlay["surface"]
        .as_array()
        .map(|held| held.iter().filter_map(|one| one["id"].as_str()).collect())
        .unwrap_or_default();
    let source = include_str!("completeness.rs");

    let mut missing: Vec<String> = Vec::new();
    for section in ["types", "members", "helpers"] {
        for (sid, per_language) in surface[section]
            .as_object()
            .expect("a section is an object")
        {
            let Some(name) = per_language["rust"].as_str() else {
                assert!(
                    waived.contains(&sid.as_str()),
                    "{sid} has no rust name and the overlay does not decline it"
                );
                continue;
            };
            let leaf = name.rsplit('.').next().expect("a name has a last segment");
            if !source.contains(leaf) {
                missing.push(format!("{sid} -> {name}"));
            }
        }
    }
    assert!(
        missing.is_empty(),
        "the surface table names rows this file does not pin: {missing:?}"
    );
}
