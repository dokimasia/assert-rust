// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `prop::engine::bridge`, which decodes a fuzzer's bytes into
//! the choices of one case.

use dokimi_assert::prop;
use dokimi_assert::prop::engine::choice::Choice;
use dokimi_assert::prop::engine::harness;

mod bridging {
    use super::*;

    #[test]
    fn reads_an_integer_from_the_bytes_that_cover_its_range() {
        let (choices, value) = harness::bridge(&prop::integer(0..=1000_u16), &[0xE8, 0x03]);
        assert_eq!(choices, [Choice::Integer(1000)]);
        assert_eq!(value, Some(1000));
        let (_, wrapped) = harness::bridge(&prop::integer(0..=1000_u16), &[0xE9, 0x03]);
        assert_eq!(wrapped, Some(0));
    }

    #[test]
    fn reads_no_byte_for_float_bounds_of_one_value() {
        let (choices, value) = harness::bridge(&prop::float(2.0..=2.0_f64), &[]);
        assert_eq!(choices, [Choice::Float(2.0)]);
        assert_eq!(value, Some(2.0));
    }

    #[test]
    fn reads_a_float_from_the_bytes_of_its_width() {
        let (_, half) = harness::bridge(
            &prop::float(0.0..=1.0_f32),
            &0.5_f32.to_bits().to_le_bytes(),
        );
        assert_eq!(half, Some(0.5));
        let (_, outside) = harness::bridge(
            &prop::float(0.0..=1.0_f64),
            &2.0_f64.to_bits().to_le_bytes(),
        );
        assert_eq!(outside, Some(0.0));
    }

    #[test]
    fn takes_the_target_of_a_value_whose_bytes_ran_out() {
        let (choices, value) = harness::bridge(&prop::float(1.0..=2.0_f64), &[1, 2, 3]);
        assert_eq!(choices, [Choice::Float(1.0)]);
        assert_eq!(value, Some(1.0));
        let (choices, bytes) = harness::bridge(&prop::bytes(1..), &[7]);
        assert_eq!(choices, [Choice::Sequence(vec![0])]);
        assert_eq!(bytes, Some(vec![0]));
    }
}
