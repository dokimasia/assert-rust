//! The spec of `prop::engine::collection`, the sizes and the decode of a
//! collection.

use dokimi_assert::prop;
use dokimi_assert::prop::engine::collection::Sizes;
use dokimi_assert::prop::engine::harness;
use std::ops::Bound;

mod sizes {
    use super::*;

    #[test]
    fn admits_no_maximum_below_the_minimum() {
        assert_eq!(Sizes::new(3, Some(1)), None);
        let sizes = Sizes::new(1, Some(3)).expect("the sizes admit a length");
        assert_eq!((sizes.min_size(), sizes.max_size()), (1, Some(3)));
    }

    #[test]
    fn reads_an_excluded_start_of_a_range_as_one_more() {
        let lists = prop::list(prop::boolean(), (Bound::Excluded(1), Bound::Included(3)));
        let lengths: Vec<usize> = (0..30)
            .filter_map(|index| harness::generate(&lists, 2, index).1)
            .map(|list| list.len())
            .collect();
        assert!(
            lengths.iter().all(|length| (2..=3).contains(length)),
            "{lengths:?}"
        );
    }
}
