//! The spec of `prop::engine::draw`, which draws each value of a generated
//! case from its random source.

use dokimi_assert::prop;
use dokimi_assert::prop::engine::draw::Ratio;
use dokimi_assert::prop::engine::harness;

mod ratio {
    use super::*;

    #[test]
    fn admits_a_probability_from_0_to_1() {
        assert!(Ratio::new(1, 2).is_some());
        assert_eq!(Ratio::new(2, 4), Ratio::new(1, 2));
        assert_eq!(Ratio::new(3, 2), None);
        assert_eq!(Ratio::new(0, 0), None);
    }
}

mod float_value {
    use super::*;

    #[test]
    fn replaces_assembled_nan_bits_with_the_target_or_the_canonical_nan() {
        let strict = prop::float(0.0..=1.0_f32);
        let values: Vec<f32> = (0..3000)
            .filter_map(|index| harness::generate(&strict, 8, index).1)
            .collect();
        assert!(values.iter().all(|x| (0.0..=1.0).contains(x)));
        let lenient = prop::float(0.0..=1.0_f32).allow_nan();
        let nans = (0..3000)
            .filter_map(|index| harness::generate(&lenient, 8, index).1)
            .filter(|x| x.is_nan())
            .count();
        assert!(nans > 0);
    }

    #[test]
    fn assembles_the_value_of_a_range_without_an_integer() {
        let infinite = prop::float(f64::INFINITY..=f64::INFINITY);
        let values: Vec<f64> = (0..50)
            .filter_map(|index| harness::generate(&infinite, 8, index).1)
            .collect();
        assert!(values.iter().all(|x| *x == f64::INFINITY), "{values:?}");
    }
}
