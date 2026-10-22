// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `prop::arbitrary::uuid`, the UUIDs of uuid.

use super::{converts, generates, refusal, shape};
use ::uuid::Uuid;
use dokimi_assert::Value;
use serde_json::json;

#[test]
fn states_the_shape_of_a_uuid() {
    assert_eq!(shape::<Uuid>(), json!({"shape": "uuid"}));
}

#[test]
fn converts_a_uuid_to_its_sixteen_bytes_and_back() {
    let id = Uuid::from_bytes([7; 16]);
    converts(&id, &Value::Bytes(vec![7; 16]));
}

#[test]
fn refuses_a_value_that_is_no_uuid() {
    assert!(refusal::<Uuid>(Value::Bytes(vec![7; 15])).ends_with("is not sixteen bytes"));
    assert!(refusal::<Uuid>(Value::Int(1)).ends_with("is not the bytes of a UUID"));
}

#[test]
fn generates_values_that_convert_back() {
    generates::<Uuid>();
}
