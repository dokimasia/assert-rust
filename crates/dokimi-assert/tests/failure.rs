// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `failure`, the record that a failing assertion reports.

use crate::helpers::failure_of;
use dokimi_assert::{Value, Where, check};
use std::panic::Location;
use std::path::Path;

mod location {
    use super::*;

    #[test]
    fn states_the_file_and_the_line_of_the_caller() {
        let line = line!() + 1;
        let failure = failure_of(|t| check::length(t, "ab", 3, "the code has three characters"));
        let at = failure.location().expect("a failure states its call site");
        assert!(Path::new(at.file()).ends_with("failure.rs"), "{at}");
        assert_eq!(at.line(), line);
    }
}

mod field {
    use super::*;

    #[test]
    fn returns_the_value_of_a_declared_field() {
        let failure = failure_of(|t| check::length(t, "ab", 3, "c"));
        assert_eq!(failure.field("got"), Some(&Value::Int(2)));
        assert_eq!(failure.want(), Some(&Value::Int(3)));
        assert_eq!(failure.got(), Some(&Value::Int(2)));
    }

    #[test]
    fn returns_none_for_a_field_that_the_assertion_does_not_declare() {
        let failure = failure_of(|t| check::is_true(t, false, "the flag is set"));
        assert_eq!(failure.field("want"), None);
        assert_eq!(failure.want(), None);
        assert_eq!(failure.got(), None);
    }

    #[test]
    fn returns_null_for_a_declared_field_without_a_value() {
        let failure = failure_of(|t| check::matches(t, "abc", "x", "c"));
        assert_eq!(failure.field("reason"), Some(&Value::Null));
    }
}

mod detail {
    use super::*;

    #[test]
    fn returns_the_fields_in_the_order_that_the_definition_declares() {
        let failure = failure_of(|t| check::in_range(t, 5, 0.0, 1.0, "c"));
        let names: Vec<&str> = failure.detail().iter().map(|(name, _)| *name).collect();
        assert_eq!(names, ["got", "low", "high"]);
    }
}

mod display {
    use super::*;

    #[test]
    fn renders_the_contract_then_each_field_in_order() {
        let failure = failure_of(|t| check::length(t, "ab", 3, "the count is right"));
        assert_eq!(failure.to_string(), "the count is right: want 3, got 2");
    }

    #[test]
    fn renders_the_contract_alone_for_a_record_without_fields() {
        let failure = failure_of(|t| check::is_true(t, false, "the flag is set"));
        assert_eq!(failure.to_string(), "the flag is set");
    }
}

mod case_failure {
    use super::*;

    #[test]
    fn returns_none_for_a_failure_of_an_assertion_outside_a_property() {
        let failure = failure_of(|t| check::is_true(t, false, "c"));
        assert_eq!(failure.case_failure(), None);
    }
}

mod r#where {
    use super::*;

    #[test]
    fn renders_the_file_and_the_line() {
        let at = Where::from(Location::caller());
        assert_eq!(at.to_string(), format!("{}:{}", at.file(), at.line()));
        assert_eq!(at.line(), line!() - 2);
    }
}
