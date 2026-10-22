//! [`Arbitrary`] for [`Decimal`], behind the feature `rust_decimal`.

use super::{Arbitrary, Definitions, mismatch};
use crate::prop::shape::{Shape, ShapeError};
use crate::value::Value;
use ::rust_decimal::Decimal;

/// A decimal reads as a decimal, whose scale the place of its use states,
/// such as `#[prop(scale = 2)]` on a field. Its value is the text of its
/// digits, such as `-12.34`.
impl Arbitrary for Decimal {
    fn shape(_: &mut Definitions) -> Shape {
        Shape::decimal()
    }

    fn from_value(value: Value) -> Result<Self, ShapeError> {
        match &value {
            Value::String(text) => Decimal::from_str_exact(text)
                .map_err(|_| mismatch("a decimal that a Decimal states", &value)),
            other => Err(mismatch("the text of a decimal", other)),
        }
    }

    fn to_value(&self) -> Value {
        Value::String(self.to_string())
    }
}
