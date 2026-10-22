// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `prop::registry`: the registrations of the test process, the
//! generator of a type and the shape file of a type.
//!
//! A registration applies to the whole process, so each case registers types
//! of its own.

use crate::helpers::{panic_of, records};
use dokimi_assert::__private::{convert_from, convert_to};
use dokimi_assert::prop::engine::harness;
use dokimi_assert::prop::{self, Arbitrary, Definitions, Shape, ShapeError, Variant};
use dokimi_assert::{Collector, Recorder, Value, check};
use serde_json::json;

/// Declares a type that wraps a byte, with the shape of a byte.
macro_rules! byte_type {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq)]
        struct $name(u8);

        impl Arbitrary for $name {
            fn shape(_: &mut Definitions) -> Shape {
                Shape::int(8, false)
            }

            fn from_value(value: Value) -> Result<Self, ShapeError> {
                u8::from_value(value).map($name)
            }

            fn to_value(&self) -> Value {
                self.0.to_value()
            }
        }
    };
}

byte_type!(Registered);
byte_type!(Listed);
byte_type!(Twice);
byte_type!(Late);
byte_type!(Unlisted);
byte_type!(Aliased);
byte_type!(Converted);
byte_type!(Kept);

/// A signal that goes, stops or waits. Its registration states the first
/// two values alone.
#[derive(Debug, Clone, PartialEq)]
enum Signal {
    Go,
    Stop,
    Wait,
}

impl Arbitrary for Signal {
    fn shape(_: &mut Definitions) -> Shape {
        Shape::boolean()
    }

    fn from_value(value: Value) -> Result<Self, ShapeError> {
        bool::from_value(value).map(|go| if go { Signal::Go } else { Signal::Stop })
    }

    fn to_value(&self) -> Value {
        Value::Bool(*self == Signal::Go)
    }
}

/// A type whose shape is the shape of a registered type.
#[derive(Debug, Clone, PartialEq)]
struct Alias(Aliased);

impl Arbitrary for Alias {
    fn shape(definitions: &mut Definitions) -> Shape {
        definitions.shape::<Aliased>()
    }

    fn from_value(value: Value) -> Result<Self, ShapeError> {
        Aliased::from_value(value).map(Alias)
    }

    fn to_value(&self) -> Value {
        self.0.to_value()
    }
}

/// A light that is red, or green for a number of seconds.
#[derive(Debug, Clone, PartialEq)]
enum Light {
    Red,
    Green(u8),
}

impl Arbitrary for Light {
    fn shape(_: &mut Definitions) -> Shape {
        Shape::boolean()
    }

    fn from_value(value: Value) -> Result<Self, ShapeError> {
        bool::from_value(value).map(|green| if green { Light::Green(1) } else { Light::Red })
    }

    fn to_value(&self) -> Value {
        Value::Bool(matches!(self, Light::Green(_)))
    }
}

/// A type that holds a registered type.
#[derive(Debug, Clone, PartialEq)]
struct Holder(Registered);

impl Arbitrary for Holder {
    fn shape(definitions: &mut Definitions) -> Shape {
        Shape::record([("held", definitions.shape::<Registered>())])
    }

    fn from_value(value: Value) -> Result<Self, ShapeError> {
        let Value::Record(mut fields) = value else {
            return Err(ShapeError::new("a holder is a record"));
        };
        Registered::from_value(fields.remove(0).1).map(Holder)
    }

    fn to_value(&self) -> Value {
        Value::Record(vec![("held".to_owned(), self.0.to_value())])
    }
}

/// Returns the call record of a run of `body` with the seed 1.
fn run(body: impl Fn(&prop::Case<'_>) + Send + Sync) -> serde_json::Value {
    let seat = Recorder::new();
    prop::for_all(
        &seat,
        "the registry holds",
        prop::hermetic().seed(1).store(""),
        body,
    );
    records(&seat).remove(0)
}

mod register {
    use super::*;

    #[test]
    fn makes_the_generator_of_a_type_wherever_it_occurs() {
        prop::register(prop::integer(200..=210_u8).map(Registered));
        let t = Collector::new();
        prop::for_all(
            &t,
            "a registered byte is 200 or more",
            prop::hermetic().seed(1),
            |c| {
                let held = c.draw(&prop::of::<Holder>(), "holder");
                check::is_true(
                    c,
                    held.0.0 >= 200,
                    "the held byte comes from the registration",
                );
                let direct = c.draw(&prop::of::<Registered>(), "byte");
                check::is_true(c, direct.0 >= 200, "the byte comes from the registration");
            },
        );
        let refused = prop::shape_of::<Holder>().expect_err("a registered generator has no shape");
        assert!(
            refused
                .to_string()
                .ends_with("has a registered generator, which has no shape"),
            "{refused}"
        );
        let record = run(|c| {
            let held = c.draw(&prop::of::<Holder>(), "holder");
            check::is_true(c, held.0.0 < 205, "the held byte stays below 205");
        });
        assert_eq!(record["detail"]["outcome"], "counterexample");
        let choices = harness::invert(
            &prop::of::<Holder>(),
            &Value::Record(vec![("held".to_owned(), Value::Int(201))]),
        );
        assert!(choices.is_err(), "a mapped generator has no inverse");
    }

    #[test]
    fn decodes_a_ref_to_a_registered_type_with_its_registered_generator() {
        prop::register(prop::integer(3..=5_u8).map(Aliased));
        let aliases = prop::of::<Alias>();
        assert_eq!(format!("{aliases:?}"), r#"Generator("map")"#);
        let values: Vec<Alias> = (0..10)
            .filter_map(|index| harness::generate(&aliases, 1, index).1)
            .collect();
        assert!(
            values.iter().all(|alias| (3..=5).contains(&alias.0.0)),
            "{values:?}"
        );
    }

    #[test]
    fn converts_a_value_through_its_registration() {
        prop::register(prop::integer(0..=9_u8).map(Converted));
        assert_eq!(
            convert_from::<Converted>(Value::Int(4)).ok(),
            Some(Converted(4))
        );
        assert_eq!(convert_to(&Converted(4)), Value::Int(4));
    }

    #[test]
    fn panics_for_a_second_registration_of_a_type() {
        prop::register(prop::integer(0..=1_u8).map(Twice));
        let raised = panic_of(|| prop::register(prop::integer(0..=1_u8).map(Twice)))
            .expect("the second registration panics");
        assert!(raised.ends_with("Twice a second time"), "{raised}");
    }

    #[test]
    fn panics_for_a_registration_after_a_read() {
        let _ = prop::of::<Late>();
        let raised = panic_of(|| prop::register(prop::integer(0..=1_u8).map(Late)))
            .expect("the late registration panics");
        assert!(
            raised.ends_with("Late after a read of the registry used its shape"),
            "{raised}"
        );
    }
}

mod register_values {
    use super::*;

    #[test]
    fn makes_the_shape_a_literal_over_the_values() {
        prop::register_values([Listed(3), Listed(1)]);
        let text = prop::shape_of::<Listed>().expect("the shape reads");
        let shape: serde_json::Value = serde_json::from_str(&text).expect("a shape file is JSON");
        assert_eq!(shape["shape"], "literal");
        assert_eq!(
            shape["values"],
            json!([{"type": "int", "value": 3}, {"type": "int", "value": 1}])
        );
        let values: Vec<Listed> = (0..20)
            .filter_map(|index| harness::generate(&prop::of::<Listed>(), 1, index).1)
            .collect();
        assert!(
            values
                .iter()
                .all(|value| *value == Listed(3) || *value == Listed(1))
        );
    }

    #[test]
    fn refuses_a_value_that_no_registered_value_states() {
        prop::register_values([Kept(1), Kept(2)]);
        let refused = convert_from::<Kept>(Value::Int(3)).expect_err("3 is no registered value");
        assert!(
            refused
                .to_string()
                .ends_with("the value 3 is not a registered value"),
            "{refused}"
        );
        assert_eq!(convert_from::<Kept>(Value::Int(2)).ok(), Some(Kept(2)));
    }

    #[test]
    fn panics_for_no_value_or_two_values_of_one_literal() {
        let raised =
            panic_of(|| prop::register_values(Vec::<Unlisted>::new())).expect("no value panics");
        assert!(raised.ends_with("states no value"), "{raised}");
        let raised = panic_of(|| prop::register_values([Unlisted(1), Unlisted(1)]))
            .expect("a repeat panics");
        assert!(
            raised.ends_with("two values state the typed literal 1"),
            "{raised}"
        );
    }
}

mod register_variants {
    use super::*;

    #[test]
    fn makes_the_shape_an_enum_of_the_variants() {
        prop::register_variants([
            Variant::unit("red", || Light::Red, |light| *light == Light::Red),
            Variant::of("green", Light::Green, |light| match light {
                Light::Green(seconds) => Some(seconds),
                Light::Red => None,
            }),
        ]);
        let text = prop::shape_of::<Light>().expect("the shape reads");
        let shape: serde_json::Value = serde_json::from_str(&text).expect("a shape file is JSON");
        assert_eq!(shape["shape"], "enum");
        assert_eq!(shape["variants"][0], json!(["red", null]));
        let lights: Vec<Light> = (0..30)
            .filter_map(|index| harness::generate(&prop::of::<Light>(), 1, index).1)
            .collect();
        assert!(lights.contains(&Light::Red));
        assert!(lights.iter().any(|light| matches!(light, Light::Green(_))));
        let variant = Variant::unit("red", || Light::Red, |light| *light == Light::Red);
        assert_eq!(format!("{variant:?}"), r#"Variant { name: "red", .. }"#);
        let record = run(|c| {
            let light = c.draw(&prop::of::<Light>(), "light");
            check::is_true(c, light == Light::Red, "the light is red");
        });
        assert_eq!(
            record["detail"]["counterexample"][0]["value"]["name"],
            "green"
        );
    }

    #[test]
    fn converts_the_values_of_its_variants_alone() {
        prop::register_variants([
            Variant::unit("go", || Signal::Go, |signal| *signal == Signal::Go),
            Variant::unit("stop", || Signal::Stop, |signal| *signal == Signal::Stop),
        ]);
        let waiting = Value::Variant {
            name: "wait".to_owned(),
            payload: None,
        };
        let refused = convert_from::<Signal>(waiting).expect_err("wait is no registered variant");
        assert!(
            refused
                .to_string()
                .ends_with(r#""wait" is no registered variant"#),
            "{refused}"
        );
        assert_eq!(
            convert_to(&Signal::Go),
            Value::Variant {
                name: "go".to_owned(),
                payload: None
            }
        );
        assert_eq!(convert_to(&Signal::Wait), Value::Bool(false));
    }

    #[test]
    fn panics_for_no_variant_or_a_name_twice() {
        let raised = panic_of(|| prop::register_variants(Vec::<Variant<Unlisted>>::new()))
            .expect("no variant panics");
        assert!(raised.ends_with("states no variant"), "{raised}");
        let raised = panic_of(|| {
            prop::register_variants([
                Variant::unit("one", || Unlisted(1), |_| true),
                Variant::unit("one", || Unlisted(2), |_| true),
            ]);
        })
        .expect("a repeat panics");
        assert!(
            raised.ends_with(r#"names the variant "one" twice"#),
            "{raised}"
        );
    }
}

mod of {
    use super::*;

    #[test]
    fn panics_for_a_type_whose_shape_does_not_read() {
        let raised = panic_of(|| drop(prop::of::<Value>())).expect("a value has no shape");
        assert!(
            raised
                .starts_with("prop::of::<dokimi_assert::value::Value>: prop: a Value has no shape"),
            "{raised}"
        );
    }

    #[test]
    fn states_the_kind_of_the_generator_of_the_shape() {
        assert_eq!(
            format!("{:?}", prop::of::<bool>()),
            r#"Generator("boolean")"#
        );
    }
}
