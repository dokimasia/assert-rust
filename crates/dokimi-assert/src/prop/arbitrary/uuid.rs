//! [`Arbitrary`] for [`Uuid`], behind the feature `uuid`.

use super::{Arbitrary, Definitions, mismatch};
use crate::prop::shape::{Shape, ShapeError};
use crate::value::Value;
use ::uuid::Uuid;

/// A UUID reads as a UUID, and its value is its sixteen bytes.
impl Arbitrary for Uuid {
    fn shape(_: &mut Definitions) -> Shape {
        Shape::uuid()
    }

    fn from_value(value: Value) -> Result<Self, ShapeError> {
        match &value {
            Value::Bytes(bytes) => {
                Uuid::from_slice(bytes).map_err(|_| mismatch("sixteen bytes", &value))
            }
            other => Err(mismatch("the bytes of a UUID", other)),
        }
    }

    fn to_value(&self) -> Value {
        Value::Bytes(self.as_bytes().to_vec())
    }
}
