//! The spec of `matcher/order`: how neighbouring items relate.

use crate::helpers::{failure_of, opaque, passes};
use dokimi_assert::{Value, check, soft};
use std::cell::Cell;

mod pairwise {
    use super::*;

    #[test]
    fn passes_items_that_increase() {
        passes(|t| check::pairwise(t, &[1, 2, 3], |a, b| a < b, "c"));
    }

    #[test]
    fn passes_no_item_or_one_item() {
        passes(|t| check::pairwise(t, &[] as &[i32], |_, _| false, "c"));
        passes(|t| check::pairwise(t, &[1], |_, _| false, "c"));
    }

    #[test]
    fn states_the_first_pair_that_fails() {
        let failure = failure_of(|t| check::pairwise(t, &[1, 3, 2, 0], |a, b| a < b, "c"));
        assert_eq!(failure.assertion(), Some("pairwise"));
        assert_eq!(
            failure.detail(),
            [
                ("index", Value::Int(1)),
                ("first", opaque("3")),
                ("second", opaque("2"))
            ]
        );
    }

    #[test]
    fn calls_the_predicate_no_more_after_the_pair_that_fails() {
        let calls = Cell::new(0);
        let increases = |a: &i32, b: &i32| {
            calls.set(calls.get() + 1);
            a < b
        };
        let _ = failure_of(|t| soft::pairwise(t, &[2, 1, 0, -1], increases, "c"));
        assert_eq!(calls.get(), 1);
    }
}
