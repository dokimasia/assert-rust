//! Every assertion the standard names exists, under the name the naming table gives it.
//!
//! The other implementations of this standard ask their runtime whether a member exists:
//! Python reaches for `getattr`, Java for reflection, TypeScript for the keys of a module.
//! Rust can ask nothing at run time, so the gate is a compile-time one instead, which is
//! the stronger of the two: every name below is referenced as a value of its own type, so
//! renaming an assertion or changing its shape fails the build rather than a test.
//!
//! What is checked at run time is only that this file has not fallen behind the table.

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
