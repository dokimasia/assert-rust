//! The spec of `dokimi-assert-derive`, whose derive writes the shape of a
//! struct or an enum and the conversions between the type's values and the
//! values of its shape.
//!
//! The derive runs when this binary compiles, so each case reads what the
//! derive wrote through `dokimi-assert`. Each misuse compiles in a project of
//! its own, and its error is compared with the file beside it under
//! `testdata/ui`.

use dokimi_assert::prop::{self, Arbitrary};
use dokimi_assert::{Collector, Value, check};
use serde_json::{Value as Json, json};
use std::net::{IpAddr, Ipv4Addr};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// A struct with a constraint of every kind that a field takes.
#[derive(Debug, Clone, PartialEq, Arbitrary)]
struct Order {
    #[prop(min = 1, max = 99)]
    quantity: u32,
    #[prop(min_size = 1, max_size = 3, alphabet = "ab")]
    code: String,
    #[prop(rename = "note", pattern = "[a-z]{0,4}")]
    remark: String,
    #[prop(allow_nan, allow_infinity)]
    ratio: f64,
    #[prop(unit = "ms")]
    placed: SystemTime,
    #[prop(version = 4)]
    from: IpAddr,
    r#type: bool,
}

/// A tuple struct, whose fields are named by position.
#[derive(Debug, Clone, PartialEq, Arbitrary)]
struct Point(i8, #[prop(max = 3)] u8);

/// An enum with a variant of each form.
#[derive(Debug, Clone, PartialEq, Arbitrary)]
enum Payment {
    #[prop(rename = "pending")]
    Pending,
    Paid(#[prop(min = 1)] i64),
    Refund {
        amount: u16,
        r#final: bool,
    },
    Split(u8, u8),
}

/// A type that refers to itself.
#[derive(Debug, Clone, PartialEq, Arbitrary)]
struct Tree {
    value: u8,
    children: Vec<Tree>,
}

/// A generic type, whose parameter the derive bounds by `Arbitrary`.
#[derive(Debug, Clone, PartialEq, Arbitrary)]
struct Pair<T> {
    first: T,
    second: T,
}

/// Returns the shape file of `T` as JSON, without its source.
fn shape<T: Arbitrary>() -> Json {
    let text = prop::shape_of::<T>().expect("the shape of the type reads");
    let mut file: Json = serde_json::from_str(&text).expect("a shape file is JSON");
    file.as_object_mut()
        .expect("a shape file is an object")
        .remove("source");
    file
}

/// Returns the value of `T` that `value` converts to, after a conversion of
/// `value` to the value of its shape.
fn round_trip<T: Arbitrary>(value: &T) -> T {
    T::from_value(value.to_value()).expect("the value of a value converts back")
}

/// Returns an order with a value in each field.
fn order() -> Order {
    Order {
        quantity: 3,
        code: "ab".to_owned(),
        remark: "note".to_owned(),
        ratio: 0.5,
        placed: UNIX_EPOCH + Duration::from_millis(1500),
        from: IpAddr::V4(Ipv4Addr::LOCALHOST),
        r#type: true,
    }
}

mod derive_arbitrary {
    use super::*;

    #[test]
    fn writes_a_record_of_the_fields_with_their_constraints() {
        let want = json!({
            "shape": "record",
            "fields": [
                ["quantity", {"shape": "int", "width": 32, "signed": false, "min": 1, "max": 99}],
                ["code", {"shape": "string", "min_size": 1, "max_size": 3, "alphabet": "ab"}],
                ["note", {"shape": "string", "pattern": "[a-z]{0,4}"}],
                ["ratio", {"shape": "float", "width": 64, "allow_nan": true, "allow_infinity": true}],
                ["placed", {"shape": "instant", "unit": "ms"}],
                ["from", {"shape": "ip-address", "version": 4}],
                ["type", {"shape": "bool"}],
            ],
        });
        assert_eq!(shape::<Order>(), want);
    }

    #[test]
    fn names_the_fields_of_a_tuple_struct_by_position() {
        let want = json!({
            "shape": "record",
            "fields": [
                ["0", {"shape": "int", "width": 8, "signed": true}],
                ["1", {"shape": "int", "width": 8, "signed": false, "max": 3}],
            ],
        });
        assert_eq!(shape::<Point>(), want);
    }

    #[test]
    fn writes_an_enum_of_the_variants_in_declaration_order() {
        let byte = json!({"shape": "int", "width": 8, "signed": false});
        let want = json!({
            "shape": "enum",
            "variants": [
                ["pending", null],
                ["Paid", {"shape": "int", "width": 64, "signed": true, "min": 1}],
                ["Refund", {"shape": "record", "fields": [
                    ["amount", {"shape": "int", "width": 16, "signed": false}],
                    ["final", {"shape": "bool"}],
                ]}],
                ["Split", {"shape": "record", "fields": [["0", byte], ["1", byte]]}],
            ],
        });
        assert_eq!(shape::<Payment>(), want);
    }

    #[test]
    fn writes_a_definition_of_a_type_that_refers_to_itself() {
        let name = std::any::type_name::<Tree>();
        let want = json!({
            "shape": "ref",
            "name": name,
            "definitions": {
                name: {"shape": "record", "fields": [
                    ["value", {"shape": "int", "width": 8, "signed": false}],
                    ["children", {"shape": "list", "of": {"shape": "ref", "name": name}}],
                ]},
            },
        });
        assert_eq!(shape::<Tree>(), want);
    }

    #[test]
    fn writes_the_shape_of_the_parameter_of_a_generic_type() {
        let flag = json!({"shape": "bool"});
        let want = json!({"shape": "record", "fields": [["first", flag], ["second", flag]]});
        assert_eq!(shape::<Pair<bool>>(), want);
    }

    #[test]
    fn converts_a_struct_to_a_record_and_back() {
        let value = order();
        assert_eq!(round_trip(&value), value);
        let Value::Record(fields) = value.to_value() else {
            panic!("a struct converts to a record");
        };
        let names: Vec<&str> = fields.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(
            names,
            [
                "quantity", "code", "note", "ratio", "placed", "from", "type"
            ]
        );
    }

    #[test]
    fn converts_each_variant_to_its_value_and_back() {
        let tests = [
            (
                Payment::Pending,
                Value::Variant {
                    name: "pending".to_owned(),
                    payload: None,
                },
            ),
            (
                Payment::Paid(7),
                Value::Variant {
                    name: "Paid".to_owned(),
                    payload: Some(Box::new(Value::Int(7))),
                },
            ),
            (
                Payment::Refund {
                    amount: 2,
                    r#final: true,
                },
                Value::Variant {
                    name: "Refund".to_owned(),
                    payload: Some(Box::new(Value::Record(vec![
                        ("amount".to_owned(), Value::Int(2)),
                        ("final".to_owned(), Value::Bool(true)),
                    ]))),
                },
            ),
            (
                Payment::Split(1, 2),
                Value::Variant {
                    name: "Split".to_owned(),
                    payload: Some(Box::new(Value::Record(vec![
                        ("0".to_owned(), Value::Int(1)),
                        ("1".to_owned(), Value::Int(2)),
                    ]))),
                },
            ),
        ];
        for (give, want) in tests {
            assert_eq!(give.to_value(), want);
            assert_eq!(round_trip(&give), give);
        }
    }

    #[test]
    fn converts_a_tuple_struct_and_a_generic_struct_back() {
        assert_eq!(round_trip(&Point(-1, 3)), Point(-1, 3));
        let pair = Pair {
            first: 1_u8,
            second: 2,
        };
        assert_eq!(round_trip(&pair), pair);
        let tree = Tree {
            value: 1,
            children: vec![Tree {
                value: 2,
                children: Vec::new(),
            }],
        };
        assert_eq!(round_trip(&tree), tree);
    }

    #[test]
    fn refuses_a_record_whose_fields_differ() {
        let refused = Point::from_value(Value::Record(vec![("0".to_owned(), Value::Int(1))]))
            .expect_err("a record of one field is no point");
        assert!(refused.to_string().contains("has not the fields"));
    }

    #[test]
    fn refuses_a_variant_that_the_enum_lacks() {
        let refused = Payment::from_value(Value::Variant {
            name: "Lost".to_owned(),
            payload: None,
        })
        .expect_err("the enum has no variant Lost");
        assert_eq!(
            refused.to_string(),
            r#"prop: "Lost" is no variant of the type"#
        );
    }

    #[test]
    fn refuses_a_value_that_is_no_variant() {
        let refused = Payment::from_value(Value::Int(1)).expect_err("an int is no variant");
        assert_eq!(refused.to_string(), "prop: the value 1 is not a variant");
    }

    #[test]
    fn generates_values_that_convert_back() {
        let t = Collector::new();
        prop::for_all(
            &t,
            "a payment converts back",
            prop::hermetic().seed(1).cases(50),
            |c| {
                let payment = c.draw(&prop::of::<Payment>(), "payment");
                check::equal(
                    c,
                    round_trip(&payment),
                    payment,
                    "the payment converts back",
                );
            },
        );
    }

    #[test]
    fn refuses_each_misuse_with_its_message() {
        trybuild::TestCases::new().compile_fail("testdata/ui/*.rs");
    }
}
