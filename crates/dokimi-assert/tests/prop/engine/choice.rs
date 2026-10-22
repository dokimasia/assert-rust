//! The spec of `prop::engine::choice`: the kinds of choice, their bounds,
//! their targets and their sort keys.

use crate::helpers::records;
use dokimi_assert::prop::engine::choice::{
    Bounds, Choice, FloatBounds, IntegerBounds, NAN_BITS, SequenceBounds, Width, next_down,
    next_up, representable, same_float, simplest_float,
};
use dokimi_assert::prop::{self, Generator};
use dokimi_assert::{Recorder, check};
use std::fmt::Debug;

/// Returns the typed literal of the minimal value of `generator` for which
/// `fails` returns `true`, as the counterexample of a property states it.
fn minimal<T: Debug + 'static>(
    generator: &Generator<T>,
    fails: impl Fn(&T) -> bool + Sync,
) -> serde_json::Value {
    let seat = Recorder::new();
    prop::for_all(
        &seat,
        "no value fails",
        prop::hermetic().seed(4).store(""),
        |c| {
            let value = c.draw(generator, "value");
            check::is_true(c, !fails(&value), "no value fails");
        },
    );
    records(&seat)[0]["detail"]["counterexample"][0]["value"].clone()
}

mod integer_bounds {
    use super::*;

    #[test]
    fn refuses_bounds_without_a_value_of_one_64_bit_range() {
        let tests = [
            (IntegerBounds::new(5, 1), "integer bounds [5, 1] are empty"),
            (
                IntegerBounds::new(-1, 18_446_744_073_709_551_615),
                "integer bounds [-1, 18446744073709551615] are inside neither the signed nor the unsigned 64-bit range",
            ),
        ];
        for (give, want) in tests {
            assert_eq!(
                give.map_err(|refused| refused.to_string()),
                Err(want.to_owned())
            );
        }
    }

    #[test]
    fn targets_the_value_closest_to_zero() {
        let tests = [((-3, 5), 0), ((2, 5), 2), ((-5, -2), -2)];
        for ((lo, hi), want) in tests {
            let bounds = IntegerBounds::new(lo, hi).expect("the bounds admit a value");
            assert_eq!(bounds.target(), want);
            assert_eq!((bounds.lo(), bounds.hi()), (lo, hi));
        }
    }

    #[test]
    fn shrinks_an_integer_along_the_longer_side_below_its_target() {
        assert_eq!(
            minimal(&prop::integer(-100..=3_i64), |n: &i64| *n <= -50)["value"],
            -50
        );
    }

    #[test]
    fn coerces_a_value_outside_the_bounds_to_the_target() {
        let bounds = IntegerBounds::new(2, 5).expect("the bounds admit a value");
        assert_eq!(bounds.coerce(&Choice::Integer(4)), 4);
        assert_eq!(bounds.coerce(&Choice::Integer(9)), 2);
        assert_eq!(bounds.coerce(&Choice::Float(4.0)), 2);
    }
}

mod float_bounds {
    use super::*;

    #[test]
    fn refuses_bounds_without_a_value_of_the_width() {
        let tests = [
            (
                FloatBounds::new(f64::NAN, 1.0, false, Width::W64),
                "float bounds [NaN, 1.0] are empty",
            ),
            (
                FloatBounds::new(2.0, 1.0, false, Width::W64),
                "float bounds [2.0, 1.0] are empty",
            ),
            (
                FloatBounds::new(0.1, 1.0, false, Width::W32),
                "float bound 0.1 is not a value of width 32",
            ),
        ];
        for (give, want) in tests {
            assert_eq!(
                give.map_err(|refused| refused.to_string()),
                Err(want.to_owned())
            );
        }
    }

    #[test]
    fn admits_nan_only_when_the_bounds_allow_it() {
        let strict =
            FloatBounds::new(0.0, 1.0, false, Width::W64).expect("the bounds admit a value");
        let lenient =
            FloatBounds::new(0.0, 1.0, true, Width::W64).expect("the bounds admit a value");
        assert!(!strict.admits(f64::NAN));
        assert!(lenient.admits(f64::NAN));
        assert_eq!(
            lenient.coerce(&Choice::Float(-f64::NAN)).to_bits(),
            NAN_BITS
        );
        assert_eq!(strict.coerce(&Choice::Float(f64::NAN)), 0.0);
        assert_eq!(
            (
                lenient.lo(),
                lenient.hi(),
                lenient.allow_nan(),
                lenient.width()
            ),
            (0.0, 1.0, true, Width::W64)
        );
    }
}

mod sequence_bounds {
    use super::*;

    #[test]
    fn refuses_bounds_without_a_sequence() {
        let tests = [
            (
                SequenceBounds::new(0, 0, None),
                "a sequence with k = 0 has no element",
            ),
            (
                SequenceBounds::new(2, 3, Some(1)),
                "sequence sizes [3, 1] are empty",
            ),
        ];
        for (give, want) in tests {
            assert_eq!(
                give.map_err(|refused| refused.to_string()),
                Err(want.to_owned())
            );
        }
    }

    #[test]
    fn fits_a_recorded_sequence_to_the_bounds() {
        let bounds = SequenceBounds::new(3, 2, Some(3)).expect("the bounds admit a sequence");
        assert_eq!(
            bounds.coerce(&Choice::Sequence(vec![1, 2, 1, 2])),
            [1, 2, 1]
        );
        assert_eq!(bounds.coerce(&Choice::Sequence(vec![5])), [0, 0]);
        assert_eq!(bounds.coerce(&Choice::Integer(1)), [0, 0]);
        assert_eq!(
            (bounds.k(), bounds.min_size(), bounds.max_size()),
            (3, 2, Some(3))
        );
    }
}

mod bounds {
    use super::*;

    #[test]
    fn admits_a_value_of_its_own_kind_alone() {
        let integer = Bounds::Integer(IntegerBounds::new(0, 5).expect("the bounds admit a value"));
        assert!(integer.admits(&Choice::Integer(3)));
        assert!(!integer.admits(&Choice::Float(3.0)));
        assert!(!integer.admits(&Choice::Sequence(vec![3])));
    }

    #[test]
    fn targets_and_coerces_each_kind() {
        let float = Bounds::Float(
            FloatBounds::new(1.5, 2.5, false, Width::W64).expect("the bounds admit a value"),
        );
        assert_eq!(float.target(), Choice::Float(2.0));
        assert_eq!(float.coerce(&Choice::Integer(0)), Choice::Float(2.0));
        assert_eq!(float.as_integer(), None);
        let sequence =
            Bounds::Sequence(SequenceBounds::new(2, 1, None).expect("the bounds admit a sequence"));
        assert_eq!(sequence.target(), Choice::Sequence(vec![0]));
        assert_eq!(
            sequence.coerce(&Choice::Sequence(vec![1, 1])),
            Choice::Sequence(vec![1, 1])
        );
        let integer = IntegerBounds::new(0, 5).expect("the bounds admit a value");
        assert_eq!(Bounds::Integer(integer).as_integer(), Some(&integer));
    }
}

mod simplest_float {
    use super::*;

    #[test]
    fn returns_the_value_with_the_smallest_key() {
        let limit = 9_007_199_254_740_992.0;
        let tests = [
            ("zero", (-1.0, 1.0), 0.0),
            ("the smallest integer", (0.5, 3.0), 1.0),
            ("a negative range", (-3.5, -0.5), -1.0),
            ("an infinity", (f64::INFINITY, f64::INFINITY), f64::INFINITY),
            (
                "an integer beyond 2^53",
                (limit + 2.0, 2.0 * limit),
                limit + 2.0,
            ),
            ("the fraction with the fewest bits", (0.3, 0.4), 0.375),
            ("the one subnormal", (5e-324, 5e-324), 5e-324),
        ];
        for (name, (lo, hi), want) in tests {
            assert!(same_float(simplest_float(lo, hi), want), "{name}");
        }
        assert!(same_float(
            simplest_float(f64::from(0.3_f32), f64::from(0.4_f32)),
            0.375
        ));
    }

    #[test]
    fn shrinks_a_float_towards_the_value_with_the_smallest_key() {
        assert_eq!(
            minimal(&prop::float(0.3..=0.4_f64), |_| true)["value"],
            0.375
        );
        let infinite = minimal(
            &prop::float(f64::NEG_INFINITY..=f64::INFINITY),
            |x: &f64| x.is_infinite(),
        );
        assert!(
            infinite["value"] == "Inf" || infinite["value"] == "-Inf",
            "{infinite}"
        );
        assert_eq!(
            minimal(&prop::float(0.0..=1.0_f64).allow_nan(), |x: &f64| x
                .is_nan())["value"],
            "NaN"
        );
    }
}

mod next_up {
    use super::*;

    #[test]
    fn steps_to_the_next_value_of_the_width() {
        assert!(next_up(f64::NAN, Width::W64).is_nan());
        assert!(same_float(
            next_up(f64::INFINITY, Width::W64),
            f64::INFINITY
        ));
        assert!(same_float(next_up(-0.0, Width::W64), 5e-324));
        assert!(same_float(
            next_up(-1.0, Width::W64),
            -1.0 + f64::EPSILON / 2.0
        ));
        assert!(same_float(
            next_up(0.0, Width::W32),
            f64::from(f32::from_bits(1))
        ));
        assert!(same_float(
            next_up(1.0, Width::W32),
            f64::from(1.0_f32 + f32::EPSILON)
        ));
        assert!(same_float(
            next_down(1.0, Width::W64),
            1.0 - f64::EPSILON / 2.0
        ));
    }
}

mod same_float {
    use super::*;

    #[test]
    fn treats_nan_as_one_value_and_the_two_zeros_as_two() {
        assert!(same_float(f64::NAN, -f64::NAN));
        assert!(!same_float(0.0, -0.0));
        assert!(!same_float(f64::NAN, 0.0));
    }
}

mod representable {
    use super::*;

    #[test]
    fn admits_a_value_of_width_32_that_an_f32_states() {
        assert!(representable(0.5, Width::W32));
        assert!(!representable(0.1, Width::W32));
        assert!(representable(f64::INFINITY, Width::W32));
        assert!(representable(0.1, Width::W64));
    }
}
