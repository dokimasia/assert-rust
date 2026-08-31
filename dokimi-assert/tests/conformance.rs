//! The corpus, driven against both surfaces, and the completeness gate.
//!
//! This is what checks meaning rather than membership: the same cases run against every
//! implementation of the standard, so a library that means something different by the same
//! name fails here.
//!
//! Written with `assert!` rather than with this library. Every assertion reports through
//! one function, so a verdict written with the subject goes quiet exactly when the subject
//! does, leaving every case passing having checked nothing.

mod conformance {
    pub mod corpus;
    pub mod subject;
    pub mod value;
}

use conformance::corpus::{Case, cases};
use conformance::value::Value;
use dokimi_assert::clock::Controlled;
use dokimi_assert::seat::Recorder;
use dokimi_assert::{check, soft};
use std::sync::Arc;

/// Drives one assertion against a case's decoded arguments.
type Invoke = fn(&Recorder, &[Value], &str);

/// Read an argument the case must have stated.
fn text(args: &[Value], at: usize) -> &str {
    args[at].as_str().expect("the case states text here")
}

fn number(args: &[Value], at: usize) -> f64 {
    args[at].as_f64().expect("the case states a number here")
}

fn count(args: &[Value], at: usize) -> usize {
    match args[at] {
        Value::Int(held) => usize::try_from(held).expect("a length is not negative"),
        _ => panic!("the case states a whole number here"),
    }
}

/// A value the standard calls absent, as the `Option` Rust states absence with.
fn optional(value: &Value) -> Option<&Value> {
    match value {
        Value::Null => None,
        held => Some(held),
    }
}

fn needles(value: &Value) -> Vec<&str> {
    match value {
        Value::List(held) => held.iter().filter_map(Value::as_str).collect(),
        _ => panic!("the case states a list of text here"),
    }
}

/// Both surfaces, driving the same cases through the same arguments.
///
/// Naming each function here is what makes this a completeness gate as well as a corpus
/// run: Rust has no way to look a function up by name at run time, so a renamed or
/// deleted assertion fails to compile rather than failing a test.
fn surfaces() -> Vec<(&'static str, Vec<(&'static str, Invoke)>)> {
    vec![("check", registry_check()), ("soft", registry_soft())]
}

macro_rules! registry {
    ($name:ident, $surface:ident) => {
        fn $name() -> Vec<(&'static str, Invoke)> {
            vec![
                ("equal", |s, a, m| $surface::equal(s, &a[0], &a[1], m)),
                ("not-equal", |s, a, m| {
                    $surface::not_equal(s, &a[0], &a[1], m)
                }),
                ("true", |s, a, m| {
                    $surface::is_true(s, a[0] == Value::Bool(true), m);
                }),
                ("false", |s, a, m| {
                    $surface::is_false(s, a[0] == Value::Bool(true), m);
                }),
                ("nil", |s, a, m| $surface::is_none(s, optional(&a[0]), m)),
                ("not-nil", |s, a, m| {
                    $surface::is_some(s, optional(&a[0]), m)
                }),
                ("length", |s, a, m| {
                    $surface::length(s, &a[0], count(a, 1), m)
                }),
                ("empty", |s, a, m| $surface::is_empty(s, &a[0], m)),
                ("not-empty", |s, a, m| $surface::is_not_empty(s, &a[0], m)),
                ("contains", |s, a, m| $surface::contains(s, &a[0], &a[1], m)),
                ("not-contains", |s, a, m| {
                    $surface::not_contains(s, &a[0], &a[1], m)
                }),
                ("contains-in-order", |s, a, m| {
                    $surface::contains_in_order(s, text(a, 0), &needles(&a[1]), m);
                }),
                ("has-prefix", |s, a, m| {
                    $surface::has_prefix(s, text(a, 0), text(a, 1), m)
                }),
                ("has-suffix", |s, a, m| {
                    $surface::has_suffix(s, text(a, 0), text(a, 1), m)
                }),
                ("matches", |s, a, m| {
                    $surface::matches(s, text(a, 0), text(a, 1), m)
                }),
                ("close-to", |s, a, m| {
                    $surface::close_to(s, number(a, 0), number(a, 1), number(a, 2), m);
                }),
                ("in-range", |s, a, m| {
                    $surface::in_range(s, number(a, 0), number(a, 1), number(a, 2), m);
                }),
            ]
        }
    };
}

registry!(registry_check, check);
registry!(registry_soft, soft);

#[test]
fn the_corpus_states_something() {
    assert!(!cases().is_empty(), "the vendored corpus states cases");
}

#[test]
fn every_case_agrees_with_this_library() {
    let all = cases();
    let mut ran = 0;
    let mut skipped = 0;

    for (surface, registry) in surfaces() {
        for case in &all {
            if let Some(why) = &case.skip {
                skipped += 1;
                eprintln!("declared skip: {} ({why})", case.id);
                continue;
            }
            let held = registry.iter().find(|(id, _)| *id == case.assertion);
            if held.is_none() && case.subject.is_none() {
                // A case for an assertion no corpus argument can express
                // is covered by this crate's own tests instead.
                continue;
            }

            let seat = Recorder::new().with_clock(Arc::new(Controlled::new()));
            if let Some(named) = &case.subject {
                // A kind this language cannot build is a skip, which is
                // what the standard states for one an implementation
                // cannot make.
                let Some(kind) = conformance::subject::Kind::read(named) else {
                    skipped += 1;
                    continue;
                };
                if !conformance::subject::run(surface, &case.assertion, kind, &seat, &case.id) {
                    skipped += 1;
                    continue;
                }
            } else {
                let (_, invoke) = held.expect("a case stating values has an invoker");
                invoke(&seat, &case.args, &case.id);
            }

            if let Err(mismatch) = case.verdict(&seat) {
                panic!("{surface} disagrees with the corpus: {mismatch}");
            }
            check_where(case, &seat);
            ran += 1;
        }
    }

    assert!(ran > 0, "the corpus reached no assertion at all");
    eprintln!("corpus: {ran} case-runs across both surfaces, {skipped} declared skips");
}

/// Hold every record to naming a real call site outside the library.
///
/// A case cannot state a line: the line is wherever the caller put the call. What
/// every case can state is that the record points somewhere a reader can open, and
/// never at the machinery that built it. Both call-site bugs this standard has
/// found were of that shape.
fn check_where(case: &Case, seat: &Recorder) {
    for held in seat.failures() {
        assert!(
            held.where_at.line > 0,
            "{}: {} reported line zero",
            case.id,
            held.assertion
        );
        assert!(
            !held.where_at.file.contains("/matcher/"),
            "{}: {} points at {}, which is the library reporting its own frame",
            case.id,
            held.assertion,
            held.where_at.file
        );
    }
}

#[test]
fn the_verdict_refuses_a_case_it_cannot_read() {
    let unreadable = Case {
        id: "made-up".to_owned(),
        assertion: "equal".to_owned(),
        args: vec![],
        expect: "maybe".to_owned(),
        detail: vec![],
        subject: None,
        skip: None,
    };
    assert!(
        unreadable.verdict(&Recorder::new()).is_err(),
        "a case stating neither pass nor fail is refused"
    );
}

#[test]
fn the_verdict_refuses_a_failure_that_reported_no_record() {
    let seat = Recorder::new();
    dokimi_assert::seat::Seat::fail(&seat, "a message reported without a record");

    let case = Case {
        id: "x".to_owned(),
        assertion: "equal".to_owned(),
        args: vec![],
        expect: "fail".to_owned(),
        detail: vec![],
        subject: None,
        skip: None,
    };
    assert!(
        case.verdict(&seat).is_err(),
        "a failure that bypassed the record seam is refused"
    );
}

#[test]
fn the_verdict_refuses_a_record_holding_a_different_value() {
    let seat = Recorder::new();
    check::equal(&seat, &1_i64, &2_i64, "x");

    let wrong = Case {
        id: "x".to_owned(),
        assertion: "equal".to_owned(),
        args: vec![],
        expect: "fail".to_owned(),
        detail: vec![("want".to_owned(), Value::Int(9))],
        subject: None,
        skip: None,
    };
    assert!(
        wrong.verdict(&seat).is_err(),
        "a record whose want differs from the case is refused"
    );

    let right = Case {
        id: "x".to_owned(),
        assertion: "equal".to_owned(),
        args: vec![],
        expect: "fail".to_owned(),
        detail: vec![("want".to_owned(), Value::Int(2))],
        subject: None,
        skip: None,
    };
    assert!(
        right.verdict(&seat).is_ok(),
        "a record matching the case is accepted: {:?}",
        right.verdict(&seat)
    );
}
