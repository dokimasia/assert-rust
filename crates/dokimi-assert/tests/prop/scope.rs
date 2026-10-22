//! The spec of `prop/scope`: the generators that `using` gives types in one
//! run, and the draws of `of` that pick them.

use crate::helpers::records;
use dokimi_assert::prop::{Arbitrary, Definitions, Generator, Options, Shape, ShapeError};
use dokimi_assert::{Recorder, Value, check, prop};
use std::fmt::Debug;
use std::sync::Mutex;

/// Returns the settings of a hermetic run of 20 cases that gives `U`
/// `generator`.
fn using<U: Arbitrary + Debug>(generator: &Generator<U>) -> Options {
    prop::using(generator)
        .hermetic()
        .seed(1)
        .store("")
        .cases(20)
}

/// Returns the generator of the digits of `u16`.
fn digits() -> Generator<u16> {
    prop::integer(0..=9_u16)
}

/// A count whose shape bounds the shape of `u16` where `u16` occurs, so it
/// does not read when a run gives `u16` a generator.
#[derive(Debug, Clone, PartialEq)]
struct Bounded(u16);

impl Arbitrary for Bounded {
    fn shape(definitions: &mut Definitions) -> Shape {
        definitions.shape::<u16>().min(1)
    }

    fn from_value(value: Value) -> Result<Self, ShapeError> {
        u16::from_value(value).map(Bounded)
    }

    fn to_value(&self) -> Value {
        self.0.to_value()
    }
}

/// The debug text of the definitions of the last read of a [`Probe`].
static PROBED: Mutex<String> = Mutex::new(String::new());

/// A count whose read keeps the debug text of its definitions in
/// [`PROBED`].
#[derive(Debug, Clone, PartialEq)]
struct Probe(u16);

impl Arbitrary for Probe {
    fn shape(definitions: &mut Definitions) -> Shape {
        let shape = definitions.shape::<u16>();
        *PROBED.lock().expect("the lock is free") = format!("{definitions:?}");
        shape
    }

    fn from_value(value: Value) -> Result<Self, ShapeError> {
        u16::from_value(value).map(Probe)
    }

    fn to_value(&self) -> Value {
        self.0.to_value()
    }
}

mod using {
    use super::*;

    #[test]
    fn draws_a_stated_type_with_the_stated_generator() {
        let seat = Recorder::new();
        prop::for_all(&seat, "c", using(&digits()), |c| {
            check::is_true(c, c.draw(&prop::of::<u16>(), "n") < 10, "n is a digit");
        });
        assert!(!seat.failed(), "{}", seat.message());
    }

    #[test]
    fn replaces_an_earlier_generator_of_one_type() {
        let seat = Recorder::new();
        let options = using(&prop::just(1_u16)).using(&prop::just(2_u16));
        prop::for_all(&seat, "c", options, |c| {
            check::equal(
                c,
                c.draw(&prop::of::<u16>(), "n"),
                2,
                "n is the later value",
            );
        });
        assert!(!seat.failed(), "{}", seat.message());
    }

    #[test]
    fn generates_each_stated_type_inside_a_type_that_contains_it() {
        let seat = Recorder::new();
        prop::for_all(&seat, "c", using(&digits()), |c| {
            let ns = c.draw(&prop::of::<Vec<u16>>(), "ns");
            check::is_true(c, ns.iter().all(|n| *n < 10), "every element is a digit");
        });
        assert!(!seat.failed(), "{}", seat.message());
    }

    #[test]
    fn draws_its_own_type_inside_the_stated_generator_without_the_setting() {
        let seat = Recorder::new();
        let above = prop::of::<u16>().map(|n| n.saturating_add(100));
        prop::for_all(&seat, "c", using(&above), |c| {
            check::is_true(
                c,
                c.draw(&prop::of::<u16>(), "n") >= 100,
                "n is 100 or more",
            );
        });
        assert!(!seat.failed(), "{}", seat.message());
    }

    #[test]
    fn keeps_the_generators_of_the_other_types_inside_a_stated_generator() {
        let seat = Recorder::new();
        let widened = prop::of::<u16>().map(u32::from);
        prop::for_all(&seat, "c", using(&prop::just(7_u16)).using(&widened), |c| {
            check::equal(c, c.draw(&prop::of::<u32>(), "n"), 7, "n is the stated u16");
        });
        assert!(!seat.failed(), "{}", seat.message());
    }

    #[test]
    fn draws_a_stated_type_with_the_stated_generator_on_each_worker() {
        let seat = Recorder::new();
        prop::for_all(&seat, "c", using(&digits()).workers(2), |c| {
            check::is_true(c, c.draw(&prop::of::<u16>(), "n") < 10, "n is a digit");
        });
        assert!(!seat.failed(), "{}", seat.message());
    }

    #[test]
    fn follows_an_entry_of_draws_through_the_stated_generator() {
        let seat = Recorder::new();
        let seen = Mutex::new(Vec::new());
        let options = using(&digits())
            .draws(r#"[{"label": "n", "value": {"type": "int", "value": 5}}]"#)
            .cases(1);
        prop::for_all(&seat, "c", options, |c| {
            seen.lock()
                .expect("the lock is free")
                .push(c.draw(&prop::of::<u16>(), "n"));
        });
        assert!(!seat.failed(), "{}", seat.message());
        assert_eq!(seen.lock().expect("the lock is free")[0], 5);
    }

    #[test]
    fn explains_the_nearest_passing_value_of_a_stated_integer() {
        let seat = Recorder::new();
        prop::for_all(&seat, "c", using(&digits()), |c| {
            check::is_true(c, c.draw(&prop::of::<u16>(), "n") < 5, "n is below 5");
        });
        let record = &records(&seat)[0];
        let drawn = &record["detail"]["counterexample"][0];
        assert_eq!(
            drawn["value"],
            serde_json::json!({"type": "int", "value": 5})
        );
        assert_eq!(
            drawn["nearest-passing"],
            serde_json::json!({"type": "int", "value": 4})
        );
    }

    #[test]
    fn fails_a_case_whose_type_does_not_read_under_the_run() {
        let seat = Recorder::new();
        prop::for_all(&seat, "c", using(&digits()), |c| {
            let _ = c.draw(&prop::of::<Bounded>(), "bounded");
        });
        let case = seat.failures()[0]
            .case_failure()
            .cloned()
            .expect("the run found a counterexample");
        assert!(
            case.contract().starts_with("prop::of::<"),
            "{}",
            case.contract()
        );
    }

    #[test]
    fn refers_to_each_stated_type_in_the_definitions_of_a_read() {
        let seat = Recorder::new();
        let probe = prop::of::<Probe>();
        prop::for_all(&seat, "c", using(&digits()).cases(1), |c| {
            let _ = c.draw(&probe, "probe");
        });
        let probed = PROBED.lock().expect("the lock is free").clone();
        assert!(
            probed.contains(r#"externals: [("u16", Generator("integer"))]"#),
            "{probed}"
        );
        assert!(probed.contains(r#"using: Some({"u16"})"#), "{probed}");
    }
}

mod of {
    use super::*;

    #[test]
    fn names_the_kind_of_the_generator_of_its_type_in_its_debug_text() {
        assert_eq!(format!("{:?}", prop::of::<u8>()), r#"Generator("integer")"#);
    }

    #[test]
    fn decodes_with_the_generator_of_the_test_process_outside_a_run() {
        let digits = prop::values(&prop::of::<u16>(), 42, 20);
        assert!(digits.iter().any(|n| *n >= 10), "{digits:?}");
    }
}
