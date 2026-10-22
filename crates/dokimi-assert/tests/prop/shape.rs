//! The spec of `prop::shape`: the builder of a type's shape, its refusal, and
//! the generator of a shape file.

mod nodes;
mod reader;
mod time;
mod zone;

use dokimi_assert::Value;
use dokimi_assert::prop::engine::harness;
use dokimi_assert::prop::{self, Arbitrary, Definitions, Shape, ShapeError};
use serde_json::{Value as Json, json};
use std::cell::RefCell;

thread_local! {
    /// The shape that `Stated` states on the thread of a test.
    static STATED: RefCell<Shape> = RefCell::new(Shape::boolean());
}

/// A type whose shape is the shape that the test of its thread states.
#[derive(Debug)]
struct Stated;

impl Arbitrary for Stated {
    fn shape(_: &mut Definitions) -> Shape {
        STATED.with(|stated| stated.borrow().clone())
    }

    fn from_value(_: Value) -> Result<Self, ShapeError> {
        Ok(Self)
    }

    fn to_value(&self) -> Value {
        Value::Null
    }
}

/// Returns the shape file of a type of the shape `shape`, without its source,
/// or the text of the refusal of the shape.
fn file_of(shape: Shape) -> Result<Json, String> {
    STATED.with(|stated| *stated.borrow_mut() = shape);
    let text = prop::shape_of::<Stated>().map_err(|refused| refused.to_string())?;
    let mut file: Json = serde_json::from_str(&text).expect("a shape file is JSON");
    file.as_object_mut()
        .expect("a shape file is an object")
        .remove("source");
    Ok(file)
}

mod shape {
    use super::*;

    #[test]
    fn states_the_shape_of_each_constructor() {
        let bool_shape = json!({"shape": "bool"});
        let tests = [
            (Shape::boolean(), bool_shape.clone()),
            (
                Shape::int(16, true),
                json!({"shape": "int", "width": 16, "signed": true}),
            ),
            (Shape::float(32), json!({"shape": "float", "width": 32})),
            (Shape::character(), json!({"shape": "char"})),
            (Shape::string(), json!({"shape": "string"})),
            (Shape::bytes(), json!({"shape": "bytes"})),
            (
                Shape::list(Shape::boolean()),
                json!({"shape": "list", "of": bool_shape}),
            ),
            (
                Shape::fixed_list(Shape::boolean(), 2),
                json!({"shape": "fixed-list", "of": bool_shape, "size": 2}),
            ),
            (
                Shape::set(Shape::boolean()),
                json!({"shape": "set", "of": bool_shape}),
            ),
            (
                Shape::map(Shape::string(), Shape::boolean()),
                json!({"shape": "map", "key": {"shape": "string"}, "of": bool_shape}),
            ),
            (
                Shape::optional(Shape::boolean()),
                json!({"shape": "optional", "of": bool_shape}),
            ),
            (
                Shape::record([("a", Shape::boolean())]),
                json!({"shape": "record", "fields": [["a", bool_shape]]}),
            ),
            (
                Shape::enumeration([("A", None), ("B", Some(Shape::boolean()))]),
                json!({"shape": "enum", "variants": [["A", null], ["B", bool_shape]]}),
            ),
            (
                Shape::literal([Value::Int(1), Value::from("x")]),
                json!({"shape": "literal", "values": [{"type": "int", "value": 1}, {"type": "string", "value": "x"}]}),
            ),
            (Shape::uuid(), json!({"shape": "uuid"})),
            (Shape::ip_address(), json!({"shape": "ip-address"})),
            (
                Shape::decimal().scale(2),
                json!({"shape": "decimal", "scale": 2}),
            ),
            (Shape::instant(), json!({"shape": "instant", "unit": "ns"})),
            (Shape::date(), json!({"shape": "date"})),
            (
                Shape::time_of_day(),
                json!({"shape": "time-of-day", "unit": "ns"}),
            ),
            (
                Shape::local_date_time(),
                json!({"shape": "local-date-time", "unit": "ns"}),
            ),
            (
                Shape::duration(),
                json!({"shape": "duration", "unit": "ns"}),
            ),
            (Shape::offset(), json!({"shape": "offset"})),
            (Shape::zone(), json!({"shape": "zone"})),
            (
                Shape::zoned_date_time(),
                json!({"shape": "zoned-date-time", "unit": "ns"}),
            ),
            (
                Shape::wall_time(),
                json!({"shape": "wall-time", "unit": "ns"}),
            ),
        ];
        for (give, want) in tests {
            assert_eq!(file_of(give), Ok(want));
        }
    }

    #[test]
    fn places_a_constraint_on_the_shape_that_takes_it() {
        let tests = [
            (
                Shape::int(8, false).min(1).max(5).max(9),
                json!({"shape": "int", "width": 8, "signed": false, "min": 1, "max": 9}),
            ),
            (
                Shape::string().min_size(1).max_size(3).alphabet("ab"),
                json!({"shape": "string", "min_size": 1, "max_size": 3, "alphabet": "ab"}),
            ),
            (
                Shape::string().pattern("[a-c]+"),
                json!({"shape": "string", "pattern": "[a-c]+"}),
            ),
            (
                Shape::float(64).allow_nan().allow_infinity(),
                json!({"shape": "float", "width": 64, "allow_nan": true, "allow_infinity": true}),
            ),
            (
                Shape::duration().unit("ms"),
                json!({"shape": "duration", "unit": "ms"}),
            ),
            (
                Shape::decimal().scale(2).min("1.5"),
                json!({"shape": "decimal", "scale": 2, "min": "1.5"}),
            ),
            (
                Shape::ip_address().version(4),
                json!({"shape": "ip-address", "version": 4}),
            ),
        ];
        for (give, want) in tests {
            assert_eq!(file_of(give), Ok(want));
        }
    }

    #[test]
    fn places_a_constraint_on_the_content_of_a_container_that_does_not_take_it() {
        let byte = json!({"shape": "int", "width": 8, "signed": false, "max": 9});
        let tests = [
            (
                Shape::list(Shape::int(8, false)).max(9),
                json!({"shape": "list", "of": byte}),
            ),
            (
                Shape::fixed_list(Shape::int(8, false), 1).max(9),
                json!({"shape": "fixed-list", "of": byte, "size": 1}),
            ),
            (
                Shape::set(Shape::int(8, false)).max(9),
                json!({"shape": "set", "of": byte}),
            ),
            (
                Shape::optional(Shape::int(8, false)).max(9),
                json!({"shape": "optional", "of": byte}),
            ),
            (
                Shape::optional(Shape::list(Shape::string())).max_size(2),
                json!({"shape": "optional", "of": {"shape": "list", "of": {"shape": "string"}, "max_size": 2}}),
            ),
        ];
        for (give, want) in tests {
            assert_eq!(file_of(give), Ok(want));
        }
    }

    #[test]
    fn writes_a_bound_as_the_json_that_a_reader_in_any_language_reads() {
        let tests = [
            (
                Shape::int(64, true).min(i64::MIN),
                json!({"shape": "int", "width": 64, "signed": true, "min": "-9223372036854775808"}),
            ),
            (
                Shape::int(128, false).max(u128::MAX),
                json!({"shape": "int", "width": 128, "signed": false, "max": "340282366920938463463374607431768211455"}),
            ),
            (
                Shape::float(64)
                    .allow_infinity()
                    .min(f64::NEG_INFINITY)
                    .max(0.5),
                json!({"shape": "float", "width": 64, "allow_infinity": true, "min": "-Inf", "max": 0.5}),
            ),
            (
                Shape::float(64).allow_infinity().max(f64::INFINITY),
                json!({"shape": "float", "width": 64, "allow_infinity": true, "max": "Inf"}),
            ),
            (
                Shape::date().min("2020-01-01"),
                json!({"shape": "date", "min": "2020-01-01"}),
            ),
        ];
        for (give, want) in tests {
            assert_eq!(file_of(give), Ok(want));
        }
        assert_eq!(
            file_of(Shape::float(64).min(f64::NAN)),
            Err("prop: shape: float bounds [NaN, 1.7976931348623157e308] are empty".to_owned())
        );
    }

    #[test]
    fn fails_to_read_for_a_bound_that_is_no_number_and_no_text() {
        assert_eq!(
            file_of(Shape::int(8, false).min(true)),
            Err("prop: the bound true is no number and no text".to_owned())
        );
    }

    #[test]
    fn fails_to_read_for_a_constraint_that_applies_to_no_part_of_the_shape() {
        let tests = [
            Shape::boolean().max(1),
            Shape::list(Shape::boolean()).max(1),
            Shape::list(Shape::boolean().max(1)),
            Shape::record([("a", Shape::boolean().max(1))]),
            Shape::enumeration([("A", Some(Shape::boolean().max(1)))]),
        ];
        for give in tests {
            assert_eq!(
                file_of(give),
                Err("prop: the constraint max applies to no part of the bool shape".to_owned())
            );
        }
    }
}

mod shape_error {
    use super::*;

    #[test]
    fn states_its_reason_after_the_prefix_of_the_module() {
        assert_eq!(
            ShapeError::new("the value 1 is not text").to_string(),
            "prop: the value 1 is not text"
        );
    }
}

mod of_shape {
    use super::*;

    #[test]
    fn returns_the_generator_of_a_shape_file() {
        let generator =
            prop::of_shape(r#"{"shape": "int", "width": 8, "signed": false, "min": 3, "max": 4}"#)
                .expect("the shape file reads");
        let values: Vec<Value> = (0..20)
            .filter_map(|index| harness::generate(&generator, 1, index).1)
            .collect();
        assert!(
            values
                .iter()
                .all(|value| matches!(value, Value::Int(3 | 4))),
            "{values:?}"
        );
    }

    #[test]
    fn refuses_text_that_is_no_json() {
        let refused = prop::of_shape("{").expect_err("the text is no JSON");
        assert_eq!(
            refused.to_string(),
            "prop: the shape file is no JSON: no name of a member at byte 1"
        );
    }
}
