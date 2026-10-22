// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `prop::form`, the run of a property form: the draws of its
//! inputs, its examples, and the faults that end its call.

use crate::helpers::records;
use dokimi_assert::prop::{self, Arbitrary, Definitions, Shape, ShapeError};
use dokimi_assert::{Recorder, Value};

/// A type whose shape does not read.
#[derive(Debug)]
struct Unshaped;

impl Arbitrary for Unshaped {
    fn shape(_: &mut Definitions) -> Shape {
        Shape::boolean().max(1)
    }

    fn from_value(_: Value) -> Result<Self, ShapeError> {
        Ok(Self)
    }

    fn to_value(&self) -> Value {
        Value::Null
    }
}

/// An identifier, whose values have no typed literal.
#[derive(Debug, Clone, PartialEq)]
struct Id(u8);

impl Arbitrary for Id {
    fn shape(_: &mut Definitions) -> Shape {
        Shape::int(8, false)
    }

    fn from_value(value: Value) -> Result<Self, ShapeError> {
        u8::from_value(value).map(Id)
    }

    fn to_value(&self) -> Value {
        self.0.to_value()
    }
}

/// Returns the message with which `call` ends on a new recorder.
fn fault(call: impl FnOnce(&Recorder)) -> String {
    let seat = Recorder::new();
    call(&seat);
    seat.message()
}

mod run {
    use super::*;

    #[test]
    fn ends_the_call_for_a_type_whose_shape_does_not_read() {
        let message =
            fault(|seat| prop::is_true(seat, |_: &Unshaped| true, "c", prop::Options::default()));
        assert_eq!(
            message,
            "prop::is_true: the constraint max applies to no part of the bool shape"
        );
    }

    #[test]
    fn ends_the_call_for_options_that_do_not_read() {
        let message =
            fault(|seat| prop::is_true(seat, |_: &u8| true, "c", prop::draws("not json")));
        assert_eq!(
            message,
            "prop::is_true: draws: the entries are not a JSON array of objects"
        );
    }

    #[test]
    fn ends_the_call_for_an_example_of_another_number_of_values() {
        let tests = [
            (
                fault(|seat| {
                    prop::is_commutative(
                        seat,
                        |a: &u8, b: &u8| *a.max(b),
                        "c",
                        prop::example([1_u8]),
                    );
                }),
                "prop::is_commutative: example[0]: the example states 1 value, and the form generates 2 inputs",
            ),
            (
                fault(|seat| prop::is_true(seat, |_: &u8| true, "c", prop::example([1_u8, 2, 3]))),
                "prop::is_true: example[0]: the example states 3 values, and the form generates 1 input",
            ),
        ];
        for (got, want) in tests {
            assert_eq!(got, want);
        }
    }

    #[test]
    fn ends_the_call_for_an_example_that_the_generator_does_not_produce() {
        let generator = prop::integer(0..=10_u16);
        let message = fault(|seat| {
            prop::is_true(
                seat,
                |_: &u16| true,
                "c",
                prop::examples([300_u16]).using(&generator),
            );
        });
        assert_eq!(
            message,
            "prop::is_true: example[0][0]: the input's generator does not produce the value: the value is outside the bounds"
        );
    }

    #[test]
    fn reads_an_input_that_contains_a_stated_type_with_the_stated_generator() {
        let seat = Recorder::new();
        let options = prop::using(&prop::integer(0..=9_u16))
            .hermetic()
            .seed(1)
            .store("");
        prop::is_true(
            &seat,
            |ns: &Vec<u16>| ns.iter().all(|n| *n < 10),
            "every element is a digit",
            options,
        );
        assert!(!seat.failed(), "{}", seat.message());
    }

    #[test]
    fn runs_an_example_of_a_value_without_a_typed_literal_as_its_value() {
        let generator = prop::integer(0..=255_u8).map(Id);
        let seat = Recorder::new();
        let options = prop::examples([Id(200)])
            .using(&generator)
            .hermetic()
            .seed(1)
            .store("");
        prop::is_true(
            &seat,
            |id: &Id| id.0 < 100,
            "the id stays below 100",
            options,
        );
        let detail = &records(&seat)[0]["detail"];
        assert_eq!(detail["outcome"], "counterexample", "{detail}");
        assert_eq!(
            detail["counterexample"][0]["value"],
            serde_json::json!({"type": "opaque", "text": "Id(200)"})
        );
    }
}
