//! The spec of `prop::engine::source`, the random source of a generated
//! case.

use dokimi_assert::prop::engine::source::{Source, case_source, mix};

mod source {
    use super::*;

    #[test]
    fn draws_the_same_values_from_the_same_seed() {
        let mut a = Source::new(7);
        let mut b = Source::new(7);
        let drawn: Vec<u64> = (0..5).map(|_| a.next_u64()).collect();
        assert_eq!(drawn, (0..5).map(|_| b.next_u64()).collect::<Vec<_>>());
    }

    #[test]
    fn draws_a_value_below_a_bound_and_a_coin() {
        let mut source = Source::new(3);
        assert!((0..100).all(|_| source.below(10) < 10));
        assert!((0..100).all(|_| !source.coin(0, 1)));
        assert!((0..100).all(|_| source.coin(1, 1)));
    }
}

mod case_source {
    use super::*;

    #[test]
    fn gives_each_case_of_a_seed_a_source_of_its_own() {
        let mut first = case_source(1, 0);
        let mut second = case_source(1, 1);
        assert_ne!(first.next_u64(), second.next_u64());
        assert_eq!(case_source(1, 5).next_u64(), case_source(1, 5).next_u64());
    }
}

mod mix {
    use super::*;

    #[test]
    fn mixes_bytes_into_one_value() {
        assert_eq!(mix(b"abc"), mix(b"abc"));
        assert_ne!(mix(b"abc"), mix(b"abd"));
    }
}
