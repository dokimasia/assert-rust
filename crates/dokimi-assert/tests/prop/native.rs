// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `prop::native`, which turns the values of a shape's generator
//! into the native values that `Arbitrary` takes, and back.
//!
//! A drawn value states the typed literal of its generator's value, which
//! the walk from its native value builds. A type whose native value its shape
//! does not state has no literal, so a report states it as opaque.

use crate::helpers::records;
use dokimi_assert::prop::{self, Arbitrary, Definitions, Shape, ShapeError};
use dokimi_assert::{Recorder, Seat, Value};
use serde_json::{Value as Json, json};
use std::fmt::Debug;

/// Returns the literal that the report of a failing run states for a drawn
/// value of `T`.
fn literal_of<T: Arbitrary + Debug>() -> Json {
    let seat = Recorder::new();
    prop::for_all(
        &seat,
        "every value fails",
        prop::hermetic().seed(1).store(""),
        |c| {
            let _ = c.draw(&prop::of::<T>(), "value");
            c.fail("the value fails");
        },
    );
    records(&seat)[0]["detail"]["counterexample"][0]["value"].clone()
}

/// Declares a type of `$shape` whose value is `$value` whatever it decodes.
macro_rules! stated {
    ($name:ident, $shape:expr, $value:expr) => {
        #[derive(Debug, Clone, PartialEq)]
        struct $name;

        impl Arbitrary for $name {
            fn shape(_: &mut Definitions) -> Shape {
                $shape
            }

            fn from_value(_: Value) -> Result<Self, ShapeError> {
                Ok($name)
            }

            fn to_value(&self) -> Value {
                $value
            }
        }
    };
}

stated!(FinerInstant, Shape::instant().unit("ms"), Value::Int(1));
stated!(TextInstant, Shape::instant(), Value::from("now"));
stated!(FinerDuration, Shape::duration().unit("s"), Value::Int(1));
stated!(
    FinerLocal,
    Shape::local_date_time().unit("s"),
    Value::Int(1)
);
stated!(
    ExtraField,
    Shape::record([("a", Shape::boolean())]),
    Value::Record(vec![("b".to_owned(), Value::Bool(true))])
);
stated!(ZonedText, Shape::zoned_date_time(), Value::from("now"));
stated!(
    ZonedFields,
    Shape::zoned_date_time(),
    Value::Record(vec![
        ("when".to_owned(), Value::Int(0)),
        ("zone".to_owned(), Value::from("UTC"))
    ])
);
stated!(DecimalInt, Shape::decimal().scale(2), Value::Int(1));
stated!(DecimalWide, Shape::decimal().scale(2), Value::from("1.234"));
stated!(DecimalWord, Shape::decimal().scale(2), Value::from("one"));
stated!(
    DecimalHuge,
    Shape::decimal().scale(0),
    Value::String("1".repeat(40))
);
stated!(
    WallTime,
    Shape::wall_time().unit("s"),
    Value::Record(vec![
        ("local-date-time".to_owned(), Value::Int(86_400_000_000_000)),
        ("zone".to_owned(), Value::from("UTC"))
    ])
);
stated!(Decimal, Shape::decimal().scale(2), Value::from("-1.5"));
stated!(Unscaled, Shape::decimal().scale(0), Value::from("12"));
stated!(
    Optional,
    Shape::optional(Shape::list(Shape::map(Shape::string(), Shape::instant()))),
    Value::List(vec![Value::Map(vec![(
        Value::from("k"),
        Value::Int(1_000_000_000)
    )])])
);
stated!(
    Choice,
    Shape::enumeration([("none", None), ("some", Some(Shape::instant()))]),
    Value::Variant {
        name: "some".to_owned(),
        payload: Some(Box::new(Value::Int(0)))
    }
);

mod walk {
    use super::*;

    #[test]
    fn states_no_literal_for_a_native_value_that_the_shape_does_not_state() {
        let opaque = |text: &str| json!({"type": "opaque", "text": text});
        let tests = [
            (literal_of::<FinerInstant>(), opaque("FinerInstant")),
            (literal_of::<TextInstant>(), opaque("TextInstant")),
            (literal_of::<FinerDuration>(), opaque("FinerDuration")),
            (literal_of::<FinerLocal>(), opaque("FinerLocal")),
            (literal_of::<ExtraField>(), opaque("ExtraField")),
            (literal_of::<ZonedText>(), opaque("ZonedText")),
            (literal_of::<ZonedFields>(), opaque("ZonedFields")),
            (literal_of::<DecimalInt>(), opaque("DecimalInt")),
            (literal_of::<DecimalWide>(), opaque("DecimalWide")),
            (literal_of::<DecimalWord>(), opaque("DecimalWord")),
            (literal_of::<DecimalHuge>(), opaque("DecimalHuge")),
        ];
        for (got, want) in tests {
            assert_eq!(got, want);
        }
    }

    #[test]
    fn states_the_literal_of_the_value_that_the_shape_decodes() {
        let wall = literal_of::<WallTime>();
        assert_eq!(wall["type"], "record");
        assert_eq!(
            wall["fields"][0][1]["fields"][0],
            json!(["date", {"type": "int", "value": 1}])
        );
        assert_eq!(
            literal_of::<Decimal>(),
            json!({"type": "int", "value": -150})
        );
        assert_eq!(
            literal_of::<Unscaled>(),
            json!({"type": "int", "value": 12})
        );
        let optional = literal_of::<Optional>();
        assert_eq!(optional["type"], "list");
        assert_eq!(literal_of::<Choice>()["name"], "some");
    }
}
