// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `prop::arbitrary::rust_decimal`, the decimals of
//! `rust_decimal`.

use super::{converts, generates, refusal};
use ::rust_decimal::Decimal;
use dokimi_assert::Value;
use dokimi_assert::prop::{self, Arbitrary, Definitions, Shape, ShapeError};

/// A price: a decimal of two digits after the point.
#[derive(Debug, Clone, PartialEq)]
struct Price(Decimal);

impl Arbitrary for Price {
    fn shape(definitions: &mut Definitions) -> Shape {
        definitions.shape::<Decimal>().scale(2)
    }

    fn from_value(value: Value) -> Result<Self, ShapeError> {
        Decimal::from_value(value).map(Price)
    }

    fn to_value(&self) -> Value {
        self.0.to_value()
    }
}

#[test]
fn states_a_decimal_whose_use_states_the_scale() {
    let refused = prop::shape_of::<Decimal>().expect_err("a decimal without a scale does not read");
    assert!(refused.to_string().contains("scale"), "{refused}");
    assert!(prop::shape_of::<Price>().is_ok());
}

#[test]
fn converts_a_decimal_to_the_text_of_its_digits_and_back() {
    converts(&Decimal::new(-1234, 2), &Value::from("-12.34"));
}

#[test]
fn refuses_a_value_that_is_no_decimal() {
    assert!(
        refusal::<Decimal>(Value::from("1.2.3"))
            .ends_with("is not a decimal that a Decimal states")
    );
    assert!(refusal::<Decimal>(Value::Int(1)).ends_with("is not the text of a decimal"));
}

#[test]
fn generates_values_that_convert_back() {
    generates::<Price>();
}
