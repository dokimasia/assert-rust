//! The spec of `matcher/values`: equality, truth and absence.

use crate::helpers::{failure_of, opaque, passes};
use dokimi_assert::{check, soft};

mod equal {
    use super::*;

    #[test]
    fn passes_two_values_that_compare_equal() {
        passes(|t| check::equal(t, String::from("widget"), "widget", "c"));
    }

    #[test]
    fn states_want_and_got_as_their_debug_text() {
        let failure = failure_of(|t| check::equal(t, vec![1, 2], [1, 3], "the list is right"));
        assert_eq!(failure.assertion(), Some("equal"));
        assert_eq!(failure.contract(), "the list is right");
        assert_eq!(
            failure.detail(),
            [("want", opaque("[1, 3]")), ("got", opaque("[1, 2]"))]
        );
    }

    #[test]
    fn fails_nan_against_itself() {
        let failure = failure_of(|t| soft::equal(t, f64::NAN, f64::NAN, "c"));
        assert_eq!(failure.got(), Some(&opaque("NaN")));
    }

    #[test]
    fn passes_zero_against_negative_zero() {
        passes(|t| check::equal(t, 0.0, -0.0, "c"));
    }
}

mod not_equal {
    use super::*;

    #[test]
    fn passes_two_values_that_differ() {
        passes(|t| check::not_equal(t, 1, 2, "c"));
    }

    #[test]
    fn states_got_for_two_equal_values() {
        let failure = failure_of(|t| check::not_equal(t, "same", "same", "c"));
        assert_eq!(failure.assertion(), Some("not-equal"));
        assert_eq!(failure.detail(), [("got", opaque("\"same\""))]);
    }
}

mod is_true {
    use super::*;

    #[test]
    fn passes_true() {
        passes(|t| check::is_true(t, true, "c"));
    }

    #[test]
    fn states_no_detail_for_false() {
        let failure = failure_of(|t| check::is_true(t, false, "the flag is set"));
        assert_eq!(failure.assertion(), Some("true"));
        assert_eq!(failure.detail(), []);
    }
}

mod is_false {
    use super::*;

    #[test]
    fn passes_false() {
        passes(|t| check::is_false(t, false, "c"));
    }

    #[test]
    fn states_no_detail_for_true() {
        let failure = failure_of(|t| soft::is_false(t, true, "the flag is clear"));
        assert_eq!(failure.assertion(), Some("false"));
        assert_eq!(failure.detail(), []);
    }
}

mod is_none {
    use super::*;

    #[test]
    fn passes_none() {
        passes(|t| check::is_none(t, None::<i32>, "c"));
    }

    #[test]
    fn states_the_value_that_some_contains() {
        let failure = failure_of(|t| check::is_none(t, Some("left over"), "c"));
        assert_eq!(failure.assertion(), Some("nil"));
        assert_eq!(failure.detail(), [("got", opaque("\"left over\""))]);
    }
}

mod is_some {
    use super::*;

    #[test]
    fn passes_some() {
        passes(|t| check::is_some(t, Some(0), "c"));
    }

    #[test]
    fn states_no_detail_for_none() {
        let failure = failure_of(|t| check::is_some(t, None::<&str>, "c"));
        assert_eq!(failure.assertion(), Some("not-nil"));
        assert_eq!(failure.detail(), []);
    }
}
