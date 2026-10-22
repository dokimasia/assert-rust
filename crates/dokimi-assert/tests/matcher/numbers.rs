// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `matcher/numbers`: numbers where exact equality is the wrong
//! question.

use crate::helpers::{failure_of, passes};
use dokimi_assert::{Number, Value, check, soft};

mod number {
    use super::*;

    #[test]
    fn reads_every_integer_type_as_a_float() {
        assert_eq!(7_i8.to_f64(), Some(7.0));
        assert_eq!(7_u16.to_f64(), Some(7.0));
        assert_eq!(7_isize.to_f64(), Some(7.0));
        assert_eq!(u64::MAX.to_f64(), Some(2.0_f64.powi(64)));
        assert_eq!(1.5_f32.to_f64(), Some(1.5));
    }

    #[test]
    fn states_an_integer_as_an_int() {
        assert_eq!((-3_i64).to_value(), Value::Int(-3));
        assert_eq!(2.5_f64.to_value(), Value::Float(2.5));
    }

    #[test]
    fn states_a_u128_above_the_ints_as_its_debug_text() {
        assert_eq!(5_u128.to_value(), Value::Int(5));
        assert_eq!(u128::MAX.to_value(), Value::Opaque(u128::MAX.to_string()));
        assert_eq!(u128::MAX.to_f64(), Some(2.0_f64.powi(128)));
    }

    #[test]
    fn reads_an_int_or_a_float_value_as_a_number() {
        assert_eq!(Value::Int(4).to_f64(), Some(4.0));
        assert_eq!(Value::Float(0.5).to_f64(), Some(0.5));
        assert_eq!(Value::from("4").to_f64(), None);
        assert_eq!(Value::from("4").to_value(), Value::from("4"));
    }

    #[test]
    fn reads_through_a_reference() {
        let three = &3_u8;
        assert_eq!(Number::to_f64(&three), Some(3.0));
        assert_eq!(Number::to_value(&three), Value::Int(3));
    }
}

mod close_to {
    use super::*;

    #[test]
    fn passes_a_difference_equal_to_the_tolerance() {
        passes(|t| check::close_to(t, 1.5, 1.0, 0.5, "c"));
    }

    #[test]
    fn states_got_want_and_tolerance() {
        let failure = failure_of(|t| check::close_to(t, 3_u32, 1.0, 0.5, "c"));
        assert_eq!(failure.assertion(), Some("close-to"));
        assert_eq!(
            failure.detail(),
            [
                ("got", Value::Int(3)),
                ("want", Value::Float(1.0)),
                ("tolerance", Value::Float(0.5))
            ]
        );
    }

    #[test]
    fn fails_nan() {
        let failure = failure_of(|t| soft::close_to(t, f64::NAN, 0.0, f64::INFINITY, "c"));
        assert!(matches!(failure.got(), Some(Value::Float(got)) if got.is_nan()));
    }

    #[test]
    fn fails_two_equal_infinities() {
        let failure = failure_of(|t| check::close_to(t, f64::INFINITY, f64::INFINITY, 1.0, "c"));
        assert_eq!(failure.got(), Some(&Value::Float(f64::INFINITY)));
    }

    #[test]
    fn fails_a_value_that_is_no_number() {
        let failure = failure_of(|t| check::close_to(t, Value::from("1"), 1.0, 0.1, "c"));
        assert_eq!(failure.got(), Some(&Value::from("1")));
    }
}

mod in_range {
    use super::*;

    #[test]
    fn passes_each_bound() {
        passes(|t| check::in_range(t, 1, 1.0, 2.0, "c"));
        passes(|t| check::in_range(t, 2.0, 1.0, 2.0, "c"));
    }

    #[test]
    fn states_got_low_and_high() {
        let failure = failure_of(|t| check::in_range(t, -1_i32, 0.0, 10.0, "c"));
        assert_eq!(failure.assertion(), Some("in-range"));
        assert_eq!(
            failure.detail(),
            [
                ("got", Value::Int(-1)),
                ("low", Value::Float(0.0)),
                ("high", Value::Float(10.0))
            ]
        );
    }

    #[test]
    fn fails_nan() {
        let failure =
            failure_of(|t| soft::in_range(t, f32::NAN, f64::NEG_INFINITY, f64::INFINITY, "c"));
        assert!(matches!(failure.got(), Some(Value::Float(got)) if got.is_nan()));
    }

    #[test]
    fn fails_every_number_against_a_nan_bound() {
        let failure = failure_of(|t| check::in_range(t, 0, f64::NAN, 1.0, "c"));
        assert_eq!(failure.got(), Some(&Value::Int(0)));
    }

    #[test]
    fn fails_a_value_that_is_no_number() {
        let failure = failure_of(|t| check::in_range(t, Value::Null, 0.0, 1.0, "c"));
        assert_eq!(failure.got(), Some(&Value::Null));
    }
}
