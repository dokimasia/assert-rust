// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `matcher/sizes`: how many items a container has.

use crate::helpers::{failure_of, opaque, passes};
use dokimi_assert::{Container, Value, check, soft};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, LinkedList, VecDeque};

mod container {
    use super::*;

    #[test]
    fn counts_the_unicode_scalar_values_of_text() {
        assert_eq!("naïve".length(), Some(5));
        assert_eq!(String::from("é").length(), Some(1));
    }

    #[test]
    fn counts_the_elements_of_each_sequence() {
        assert_eq!([1, 2, 3][..].length(), Some(3));
        assert_eq!([1, 2].length(), Some(2));
        assert_eq!(vec!['a'].length(), Some(1));
        assert_eq!(VecDeque::from([1, 2]).length(), Some(2));
        assert_eq!(LinkedList::from([1, 2, 3, 4]).length(), Some(4));
    }

    #[test]
    fn counts_the_entries_of_each_map_or_set() {
        assert_eq!(HashMap::from([(1, 'a'), (2, 'b')]).length(), Some(2));
        assert_eq!(BTreeMap::from([(1, 'a')]).length(), Some(1));
        assert_eq!(HashSet::from([1, 2, 3]).length(), Some(3));
        assert_eq!(BTreeSet::<u8>::new().length(), Some(0));
    }

    #[test]
    fn counts_through_a_reference_or_a_box() {
        let items = &vec![1, 2];
        assert_eq!(Container::length(&items), Some(2));
        let boxed: Box<[i32]> = Box::new([1, 2, 3]);
        assert_eq!(boxed.length(), Some(3));
    }

    #[test]
    fn returns_no_length_for_none() {
        assert_eq!(None::<Vec<i32>>.length(), None);
        assert_eq!(Some(vec![1]).length(), Some(1));
    }

    #[test]
    fn counts_the_items_of_each_value_that_has_a_length() {
        assert_eq!(Value::List(vec![Value::Null]).length(), Some(1));
        assert_eq!(
            Value::Map(vec![(Value::Int(1), Value::Int(2))]).length(),
            Some(1)
        );
        assert_eq!(Value::from("über").length(), Some(4));
        assert_eq!(Value::Bytes(vec![0xc3, 0xbc]).length(), Some(2));
        assert_eq!(Value::Int(3).length(), None);
        assert_eq!(Value::Null.length(), None);
    }
}

mod length {
    use super::*;

    #[test]
    fn passes_a_container_with_the_count() {
        passes(|t| check::length(t, "abc", 3, "c"));
    }

    #[test]
    fn states_want_and_got_as_integers() {
        let failure = failure_of(|t| check::length(t, &[1, 2], 3, "c"));
        assert_eq!(failure.assertion(), Some("length"));
        assert_eq!(
            failure.detail(),
            [("want", Value::Int(3)), ("got", Value::Int(2))]
        );
    }

    #[test]
    fn states_null_for_a_value_without_a_length() {
        let failure = failure_of(|t| soft::length(t, &None::<String>, 0, "c"));
        assert_eq!(failure.got(), Some(&Value::Null));
    }
}

mod is_empty {
    use super::*;

    #[test]
    fn passes_an_empty_container() {
        passes(|t| check::is_empty(t, &Vec::<u8>::new(), "c"));
    }

    #[test]
    fn states_the_container_with_its_length() {
        let failure = failure_of(|t| check::is_empty(t, "ab", "c"));
        assert_eq!(failure.assertion(), Some("empty"));
        assert_eq!(
            failure.detail(),
            [("got", opaque("\"ab\"")), ("length", Value::Int(2))]
        );
    }

    #[test]
    fn states_null_for_a_value_without_a_length() {
        let failure = failure_of(|t| check::is_empty(t, &Value::Bool(true), "c"));
        assert_eq!(
            failure.detail(),
            [("got", Value::Null), ("length", Value::Null)]
        );
    }
}

mod is_not_empty {
    use super::*;

    #[test]
    fn passes_a_container_with_an_item() {
        passes(|t| check::is_not_empty(t, &[0], "c"));
    }

    #[test]
    fn states_an_empty_container() {
        let failure = failure_of(|t| check::is_not_empty(t, "", "c"));
        assert_eq!(failure.assertion(), Some("not-empty"));
        assert_eq!(failure.detail(), [("got", opaque("\"\""))]);
    }

    #[test]
    fn states_null_for_a_value_without_a_length() {
        let failure = failure_of(|t| soft::is_not_empty(t, &None::<Vec<u8>>, "c"));
        assert_eq!(failure.detail(), [("got", Value::Null)]);
    }
}
