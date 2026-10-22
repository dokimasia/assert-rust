// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `prop::arbitrary`: the shapes of the types of the standard
//! library and the conversions between their values and the native values
//! of their shapes.

#[cfg(feature = "chrono")]
mod chrono;
#[cfg(feature = "jiff")]
mod jiff;
#[cfg(feature = "rust_decimal")]
mod rust_decimal;
#[cfg(feature = "time")]
mod time;
#[cfg(feature = "uuid")]
mod uuid;

use dokimi_assert::prop::{self, Arbitrary, Definitions, Shape, ShapeError};
use dokimi_assert::{Collector, Value, check};
use serde_json::{Value as Json, json};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt::Debug;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Returns the shape file of `T` as JSON, without its source.
pub(crate) fn shape<T: Arbitrary>() -> Json {
    let text = prop::shape_of::<T>().expect("the shape of the type reads");
    let mut file: Json = serde_json::from_str(&text).expect("a shape file is JSON");
    file.as_object_mut()
        .expect("a shape file is an object")
        .remove("source");
    file
}

/// Asserts that `value` converts to `native` and back.
pub(crate) fn converts<T: Arbitrary + PartialEq + Debug>(value: &T, native: &Value) {
    assert_eq!(&value.to_value(), native, "the native value of {value:?}");
    assert_eq!(
        &T::from_value(native.clone()).expect("the native value converts back"),
        value
    );
}

/// Returns the text of the refusal of `native` as a value of `T`.
pub(crate) fn refusal<T: Arbitrary + Debug>(native: Value) -> String {
    T::from_value(native)
        .expect_err("the value is no value of the type")
        .to_string()
}

/// Asserts that every value of 50 cases of the generator of `T` converts
/// back.
pub(crate) fn generates<T: Arbitrary + PartialEq + Debug>() {
    let t = Collector::new();
    let contract = format!("a generated {} converts back", std::any::type_name::<T>());
    prop::for_all(
        &t,
        &contract,
        prop::hermetic().seed(7).cases(50).store(""),
        |c| {
            let value = c.draw(&prop::of::<T>(), "value");
            let back = T::from_value(value.to_value());
            check::is_true(c, back.as_ref() == Ok(&value), "the value converts back");
        },
    );
}

/// A type whose shape refers to a recursive type twice: a pair of trees.
#[derive(Debug, Clone, PartialEq)]
struct Forest(Tree, Tree);

impl Arbitrary for Forest {
    fn shape(definitions: &mut Definitions) -> Shape {
        <(Tree, Tree)>::shape(definitions)
    }

    fn from_value(value: Value) -> Result<Self, ShapeError> {
        <(Tree, Tree)>::from_value(value).map(|(left, right)| Forest(left, right))
    }

    fn to_value(&self) -> Value {
        (self.0.clone(), self.1.clone()).to_value()
    }
}

/// A recursive type: a tree whose value is the list of its children.
#[derive(Debug, Clone, PartialEq)]
struct Tree(Vec<Tree>);

impl Arbitrary for Tree {
    fn shape(definitions: &mut Definitions) -> Shape {
        definitions.define::<Self>(Definitions::shape::<Vec<Tree>>)
    }

    fn from_value(value: Value) -> Result<Self, ShapeError> {
        Vec::<Tree>::from_value(value).map(Tree)
    }

    fn to_value(&self) -> Value {
        self.0.to_value()
    }
}

/// A type that states its shape with the builder, and refuses a value
/// above 100.
#[derive(Debug, Clone, PartialEq)]
struct Percent(u8);

impl Arbitrary for Percent {
    fn shape(_: &mut Definitions) -> Shape {
        Shape::int(8, false).max(100)
    }

    fn from_value(value: Value) -> Result<Self, ShapeError> {
        u8::from_value(value).map(Percent)
    }

    fn to_value(&self) -> Value {
        self.0.to_value()
    }
}

mod definitions {
    use super::*;

    #[test]
    fn defines_a_recursive_type_once_and_refers_to_it_after() {
        let name = std::any::type_name::<Tree>();
        let stated = shape::<Forest>();
        assert_eq!(
            stated["fields"][0][1],
            json!({"shape": "ref", "name": name})
        );
        assert_eq!(
            stated["fields"][1][1],
            json!({"shape": "ref", "name": name})
        );
        let definition = json!({"shape": "list", "of": {"shape": "ref", "name": name}});
        assert_eq!(stated["definitions"], json!({ name: definition }));
    }

    #[test]
    fn states_the_shape_of_a_type_that_implements_the_trait_by_hand() {
        assert_eq!(
            shape::<Percent>(),
            json!({"shape": "int", "width": 8, "signed": false, "max": 100})
        );
        converts(&Percent(7), &Value::Int(7));
    }
}

mod scalars {
    use super::*;

    #[test]
    fn states_the_shape_of_each_scalar() {
        let tests = [
            (shape::<bool>(), json!({"shape": "bool"})),
            (
                shape::<i8>(),
                json!({"shape": "int", "width": 8, "signed": true}),
            ),
            (
                shape::<u16>(),
                json!({"shape": "int", "width": 16, "signed": false}),
            ),
            (
                shape::<i32>(),
                json!({"shape": "int", "width": 32, "signed": true}),
            ),
            (
                shape::<u64>(),
                json!({"shape": "int", "width": 64, "signed": false}),
            ),
            (
                shape::<usize>(),
                json!({"shape": "int", "width": usize::BITS, "signed": false}),
            ),
            (
                shape::<isize>(),
                json!({"shape": "int", "width": usize::BITS, "signed": true}),
            ),
            (
                shape::<u8>(),
                json!({"shape": "int", "width": 8, "signed": false}),
            ),
            (
                shape::<i128>(),
                json!({"shape": "int", "width": 128, "signed": true}),
            ),
            (
                shape::<u128>(),
                json!({"shape": "int", "width": 128, "signed": false}),
            ),
            (shape::<f64>(), json!({"shape": "float", "width": 64})),
            (shape::<f32>(), json!({"shape": "float", "width": 32})),
            (shape::<char>(), json!({"shape": "char"})),
            (shape::<String>(), json!({"shape": "string"})),
        ];
        for (got, want) in tests {
            assert_eq!(got, want);
        }
    }

    #[test]
    fn converts_each_scalar_to_its_native_value_and_back() {
        converts(&true, &Value::Bool(true));
        converts(&-3_i8, &Value::Int(-3));
        converts(&7_u64, &Value::Int(7));
        converts(&200_u8, &Value::Int(200));
        converts(&i128::MIN, &Value::Int(i128::MIN));
        converts(&u128::MAX, &Value::Uint(u128::MAX));
        converts(&0.5_f64, &Value::Float(0.5));
        converts(&0.25_f32, &Value::Float(0.25));
        converts(&'x', &Value::from("x"));
        converts(&"text".to_owned(), &Value::from("text"));
        assert_eq!(u128::from_value(Value::Int(5)), Ok(5));
    }

    #[test]
    fn refuses_a_value_of_another_form() {
        let tests = [
            (
                refusal::<bool>(Value::Int(1)),
                "prop: the value 1 is not a bool",
            ),
            (
                refusal::<i8>(Value::Int(300)),
                "prop: the value 300 is not an int of i8",
            ),
            (
                refusal::<i64>(Value::from("a")),
                "prop: the value \"a\" is not an int",
            ),
            (
                refusal::<u8>(Value::Int(-1)),
                "prop: the value -1 is not an int of u8",
            ),
            (
                refusal::<i128>(Value::Bool(true)),
                "prop: the value true is not an int",
            ),
            (
                refusal::<u128>(Value::Int(-1)),
                "prop: the value -1 is not an int of u128",
            ),
            (
                refusal::<u128>(Value::Float(1.0)),
                "prop: the value 1.0 is not an int of u128",
            ),
            (
                refusal::<f64>(Value::Int(1)),
                "prop: the value 1 is not a float",
            ),
            (
                refusal::<f32>(Value::Int(1)),
                "prop: the value 1 is not a float",
            ),
            (
                refusal::<char>(Value::from("xy")),
                "prop: the value \"xy\" is not one character",
            ),
            (
                refusal::<char>(Value::Int(1)),
                "prop: the value 1 is not one character",
            ),
            (
                refusal::<String>(Value::Int(1)),
                "prop: the value 1 is not text",
            ),
        ];
        for (got, want) in tests {
            assert_eq!(got, want);
        }
    }

    #[test]
    fn generates_values_that_convert_back() {
        generates::<bool>();
        generates::<i16>();
        generates::<u32>();
        generates::<u128>();
        generates::<f32>();
        generates::<char>();
        generates::<String>();
    }
}

mod collections {
    use super::*;

    #[test]
    fn states_the_shape_of_each_collection() {
        let byte = json!({"shape": "int", "width": 8, "signed": false});
        let tests = [
            (shape::<Vec<u8>>(), json!({"shape": "bytes"})),
            (
                shape::<Vec<bool>>(),
                json!({"shape": "list", "of": {"shape": "bool"}}),
            ),
            (
                shape::<[u8; 3]>(),
                json!({"shape": "fixed-list", "of": byte, "size": 3}),
            ),
            (
                shape::<Option<u8>>(),
                json!({"shape": "optional", "of": byte}),
            ),
            (shape::<Box<u8>>(), byte.clone()),
            (
                shape::<BTreeMap<u8, bool>>(),
                json!({"shape": "map", "key": byte, "of": {"shape": "bool"}}),
            ),
            (
                shape::<HashMap<u8, bool>>(),
                json!({"shape": "map", "key": byte, "of": {"shape": "bool"}}),
            ),
            (shape::<BTreeSet<u8>>(), json!({"shape": "set", "of": byte})),
            (shape::<HashSet<u8>>(), json!({"shape": "set", "of": byte})),
            (
                shape::<(u8, bool)>(),
                json!({"shape": "record", "fields": [["0", byte], ["1", {"shape": "bool"}]]}),
            ),
        ];
        for (got, want) in tests {
            assert_eq!(got, want);
        }
    }

    #[test]
    fn converts_each_collection_to_its_native_value_and_back() {
        converts(&vec![1_u8, 2], &Value::Bytes(vec![1, 2]));
        converts(&vec![true], &Value::List(vec![Value::Bool(true)]));
        converts(&[1_u8, 2], &Value::List(vec![Value::Int(1), Value::Int(2)]));
        converts(&None::<u8>, &Value::Null);
        converts(&Some(3_u8), &Value::Int(3));
        converts(&Box::new(3_u8), &Value::Int(3));
        converts(
            &BTreeMap::from([(1_u8, true)]),
            &Value::Map(vec![(Value::Int(1), Value::Bool(true))]),
        );
        converts(
            &HashMap::from([(1_u8, true)]),
            &Value::Map(vec![(Value::Int(1), Value::Bool(true))]),
        );
        converts(&BTreeSet::from([4_u8]), &Value::List(vec![Value::Int(4)]));
        converts(&HashSet::from([4_u8]), &Value::List(vec![Value::Int(4)]));
        let fields = |values: Vec<Value>| {
            Value::Record(
                values
                    .into_iter()
                    .enumerate()
                    .map(|(at, value)| (at.to_string(), value))
                    .collect(),
            )
        };
        converts(&(1_u8,), &fields(vec![Value::Int(1)]));
        converts(
            &(1_u8, true),
            &fields(vec![Value::Int(1), Value::Bool(true)]),
        );
        converts(
            &(1_u8, true, 'a'),
            &fields(vec![Value::Int(1), Value::Bool(true), Value::from("a")]),
        );
        converts(
            &(1_u8, 2_u8, 3_u8, 4_u8),
            &fields((1..=4).map(Value::Int).collect()),
        );
        converts(
            &(1_u8, 2_u8, 3_u8, 4_u8, 5_u8),
            &fields((1..=5).map(Value::Int).collect()),
        );
        converts(
            &(1_u8, 2_u8, 3_u8, 4_u8, 5_u8, 6_u8),
            &fields((1..=6).map(Value::Int).collect()),
        );
    }

    #[test]
    fn refuses_a_value_of_another_form() {
        let tests = [
            (
                refusal::<Vec<u8>>(Value::List(Vec::new())),
                "prop: the value [] is not bytes",
            ),
            (
                refusal::<Vec<bool>>(Value::Int(1)),
                "prop: the value 1 is not a list",
            ),
            (
                refusal::<[u8; 2]>(Value::Int(1)),
                "prop: the value 1 is not a list",
            ),
            (
                refusal::<[u8; 2]>(Value::List(vec![Value::Int(1)])),
                "prop: a list of 1 items is no array of 2",
            ),
            (
                refusal::<BTreeMap<u8, u8>>(Value::Int(1)),
                "prop: the value 1 is not a map",
            ),
            (
                refusal::<HashMap<u8, u8>>(Value::Int(1)),
                "prop: the value 1 is not a map",
            ),
            (
                refusal::<BTreeSet<u8>>(Value::Int(1)),
                "prop: the value 1 is not a list",
            ),
            (
                refusal::<HashSet<u8>>(Value::Int(1)),
                "prop: the value 1 is not a list",
            ),
        ];
        for (got, want) in tests {
            assert_eq!(got, want);
        }
        assert!(
            refusal::<(u8, u8)>(Value::Int(1)).starts_with("prop: the value 1 is not a record")
        );
    }

    #[test]
    fn generates_values_that_convert_back() {
        generates::<Vec<u8>>();
        generates::<[bool; 2]>();
        generates::<Option<u8>>();
        generates::<Box<u8>>();
        generates::<BTreeMap<u8, bool>>();
        generates::<HashMap<u8, bool>>();
        generates::<BTreeSet<u8>>();
        generates::<HashSet<u8>>();
        generates::<(u8, bool, char, i8, u16, i16)>();
        generates::<Forest>();
    }
}

mod times {
    use super::*;

    #[test]
    fn states_a_duration_from_0_and_a_system_time_as_an_instant() {
        assert_eq!(
            shape::<Duration>(),
            json!({"shape": "duration", "unit": "ns", "min": 0})
        );
        assert_eq!(
            shape::<SystemTime>(),
            json!({"shape": "instant", "unit": "ns"})
        );
    }

    #[test]
    fn converts_a_time_to_its_nanoseconds_and_back() {
        converts(&Duration::from_millis(1500), &Value::Int(1_500_000_000));
        converts(
            &(UNIX_EPOCH + Duration::from_secs(1)),
            &Value::Int(1_000_000_000),
        );
        converts(
            &(UNIX_EPOCH - Duration::from_millis(1500)),
            &Value::Int(-1_500_000_000),
        );
    }

    #[test]
    fn refuses_a_time_outside_its_type() {
        assert_eq!(
            refusal::<Duration>(Value::Int(-1)),
            "prop: the value -1 is not a duration of 0 or more nanoseconds"
        );
        assert!(
            refusal::<SystemTime>(Value::Int(i128::MAX))
                .ends_with("is not an instant that the system clock states")
        );
        assert_eq!(
            refusal::<Duration>(Value::from("1s")),
            "prop: the value \"1s\" is not an int"
        );
    }

    #[test]
    fn generates_values_that_convert_back() {
        generates::<Duration>();
        generates::<SystemTime>();
    }
}

mod addresses {
    use super::*;

    #[test]
    fn states_an_address_of_either_version_or_of_one() {
        assert_eq!(shape::<IpAddr>(), json!({"shape": "ip-address"}));
        assert_eq!(
            shape::<Ipv4Addr>(),
            json!({"shape": "ip-address", "version": 4})
        );
        assert_eq!(
            shape::<Ipv6Addr>(),
            json!({"shape": "ip-address", "version": 6})
        );
    }

    #[test]
    fn converts_an_address_to_its_bytes_and_back() {
        converts(
            &IpAddr::V4(Ipv4Addr::LOCALHOST),
            &Value::Bytes(vec![127, 0, 0, 1]),
        );
        converts(
            &IpAddr::V6(Ipv6Addr::LOCALHOST),
            &Value::Bytes(Ipv6Addr::LOCALHOST.octets().to_vec()),
        );
        converts(&Ipv4Addr::LOCALHOST, &Value::Bytes(vec![127, 0, 0, 1]));
        converts(
            &Ipv6Addr::LOCALHOST,
            &Value::Bytes(Ipv6Addr::LOCALHOST.octets().to_vec()),
        );
    }

    #[test]
    fn refuses_bytes_of_another_length_or_version() {
        let v6 = Value::Bytes(Ipv6Addr::LOCALHOST.octets().to_vec());
        let tests = [
            (
                refusal::<IpAddr>(Value::Bytes(vec![1; 5])),
                "prop: the value b\"\\x01\\x01\\x01\\x01\\x01\" is not an address of 4 or 16 bytes",
            ),
            (
                refusal::<IpAddr>(Value::Int(1)),
                "prop: the value 1 is not an address",
            ),
            (
                refusal::<Ipv4Addr>(v6),
                "prop: the value b\"\\x00\\x00\\x00\\x00\\x00\\x00\\x00\\x00\\x00\\x00\\x00\\x00\\x00\\x00\\x00\\x01\" is not an address of version 4",
            ),
            (
                refusal::<Ipv6Addr>(Value::Bytes(vec![127, 0, 0, 1])),
                "prop: the value b\"\\x7f\\x00\\x00\\x01\" is not an address of version 6",
            ),
        ];
        for (got, want) in tests {
            assert_eq!(got, want);
        }
    }

    #[test]
    fn generates_values_that_convert_back() {
        generates::<IpAddr>();
        generates::<Ipv4Addr>();
        generates::<Ipv6Addr>();
    }
}

mod value {
    use super::*;

    #[test]
    fn states_no_shape_of_its_own() {
        let refused = prop::shape_of::<Value>().expect_err("a Value has no shape");
        assert_eq!(
            refused.to_string(),
            "prop: a Value has no shape of its own, so its generator comes from prop::using"
        );
    }

    #[test]
    fn converts_a_value_to_itself() {
        converts(&Value::from("same"), &Value::from("same"));
    }
}
