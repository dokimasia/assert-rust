//! The spec of `matcher/containment`: what a container contains.

use crate::helpers::{failure_of, opaque, passes};
use dokimi_assert::{Contains, Value, check, soft};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, LinkedList, VecDeque};

mod contains_trait {
    use super::*;

    #[test]
    fn finds_a_substring_in_text() {
        assert!(Contains::<str>::contains("haystack", "st"));
        assert!(Contains::<str>::contains(&String::from("haystack"), "hay"));
        assert!(Contains::<String>::contains(
            &String::from("haystack"),
            &String::from("ack")
        ));
        assert!(Contains::<String>::contains("haystack", &String::from("y")));
        assert!(!Contains::<str>::contains("haystack", "needle"));
    }

    #[test]
    fn finds_an_element_in_each_sequence() {
        assert!(Contains::contains(&[1, 2, 3][..], &2));
        assert!(Contains::contains(&[1, 2, 3], &3));
        assert!(Contains::contains(&vec![1, 2], &1));
        assert!(Contains::contains(&VecDeque::from([1, 2]), &2));
        assert!(Contains::contains(&LinkedList::from([4]), &4));
        assert!(!Contains::contains(&vec![1, 2], &5));
    }

    #[test]
    fn finds_a_key_in_each_map_or_set() {
        let names = HashMap::from([(String::from("a"), 1)]);
        assert!(Contains::<str>::contains(&names, "a"));
        assert!(Contains::contains(&BTreeMap::from([(1, 'x')]), &1));
        assert!(Contains::contains(&HashSet::from([7]), &7));
        assert!(Contains::<str>::contains(
            &BTreeSet::from([String::from("k")]),
            "k"
        ));
        assert!(!Contains::<str>::contains(&names, "b"));
    }

    #[test]
    fn finds_through_a_reference() {
        let items = vec![1, 2];
        assert!(Contains::contains(&&items, &2));
    }

    #[test]
    fn finds_an_element_of_a_list_value() {
        let list = Value::List(vec![Value::Int(1), Value::from("b")]);
        assert!(list.contains(&Value::from("b")));
        assert!(!list.contains(&Value::Float(1.0)));
    }

    #[test]
    fn finds_a_key_of_a_map_value() {
        let map = Value::Map(vec![(Value::from("k"), Value::Int(1))]);
        assert!(map.contains(&Value::from("k")));
        assert!(!map.contains(&Value::Int(1)));
    }

    #[test]
    fn finds_a_substring_of_a_text_value() {
        assert!(Value::from("haystack").contains(&Value::from("ays")));
        assert!(!Value::from("haystack").contains(&Value::Int(1)));
    }

    #[test]
    fn finds_a_run_of_bytes_in_a_bytes_value() {
        let bytes = Value::Bytes(vec![1, 2, 3]);
        assert!(bytes.contains(&Value::Bytes(vec![2, 3])));
        assert!(bytes.contains(&Value::Bytes(vec![])));
        assert!(!bytes.contains(&Value::Bytes(vec![3, 2])));
    }

    #[test]
    fn finds_nothing_in_a_scalar_value() {
        assert!(!Value::Int(1).contains(&Value::Int(1)));
        assert!(!Value::Null.contains(&Value::Null));
    }
}

mod contains {
    use super::*;

    #[test]
    fn passes_a_haystack_that_contains_the_needle() {
        passes(|t| check::contains(t, "a needle", "need", "c"));
    }

    #[test]
    fn states_the_haystack_with_the_needle() {
        let failure = failure_of(|t| check::contains(t, &[1, 2], &3, "c"));
        assert_eq!(failure.assertion(), Some("contains"));
        assert_eq!(
            failure.detail(),
            [("haystack", opaque("[1, 2]")), ("needle", opaque("3"))]
        );
    }
}

mod not_contains {
    use super::*;

    #[test]
    fn passes_a_haystack_without_the_needle() {
        passes(|t| check::not_contains(t, "hay", "needle", "c"));
    }

    #[test]
    fn states_the_haystack_with_the_needle() {
        let failure = failure_of(|t| soft::not_contains(t, "hay", "a", "c"));
        assert_eq!(failure.assertion(), Some("not-contains"));
        assert_eq!(
            failure.detail(),
            [("haystack", opaque("\"hay\"")), ("needle", opaque("\"a\""))]
        );
    }
}

mod contains_in_order {
    use super::*;

    #[test]
    fn passes_needles_that_follow_one_another() {
        passes(|t| check::contains_in_order(t, "GET /items 200", &["GET", "/items", "200"], "c"));
    }

    #[test]
    fn passes_an_empty_list_of_needles() {
        passes(|t| check::contains_in_order(t, "", &[], "c"));
    }

    #[test]
    fn states_the_place_of_the_needle_that_is_missing() {
        let failure = failure_of(|t| check::contains_in_order(t, "b a", &["a", "b"], "c"));
        assert_eq!(failure.assertion(), Some("contains-in-order"));
        assert_eq!(
            failure.detail(),
            [
                ("haystack", Value::from("b a")),
                ("needle", Value::from("b")),
                ("index", Value::Int(1))
            ]
        );
    }

    #[test]
    fn fails_two_needles_that_one_occurrence_satisfies() {
        let failure = failure_of(|t| check::contains_in_order(t, "ab", &["ab", "b"], "c"));
        assert_eq!(failure.field("index"), Some(&Value::Int(1)));
    }
}

mod is_permutation {
    use super::*;

    /// A value that equals each value within one of it, so equality is not
    /// transitive.
    #[derive(Debug)]
    struct Near(i32);

    impl PartialEq for Near {
        fn eq(&self, other: &Self) -> bool {
            (self.0 - other.0).abs() <= 1
        }
    }

    #[test]
    fn passes_the_same_elements_in_another_order() {
        passes(|t| check::is_permutation(t, &[3, 1, 2, 1], &[1, 1, 2, 3], "c"));
    }

    #[test]
    fn fails_elements_that_occur_another_number_of_times() {
        let failure = failure_of(|t| check::is_permutation(t, &[1, 1, 2], &[1, 2, 2], "c"));
        assert_eq!(failure.assertion(), Some("permutation"));
        assert_eq!(
            failure.detail(),
            [("want", opaque("[1, 2, 2]")), ("got", opaque("[1, 1, 2]"))]
        );
    }

    #[test]
    fn fails_lists_of_two_lengths() {
        let failure = failure_of(|t| soft::is_permutation(t, &[1], &[1, 1], "c"));
        assert_eq!(failure.got(), Some(&opaque("[1]")));
    }

    #[test]
    fn finds_a_pairing_that_the_first_choice_of_each_element_misses() {
        passes(|t| check::is_permutation(t, &[Near(2), Near(0)], &[Near(1), Near(3)], "c"));
    }
}
