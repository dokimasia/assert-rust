// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `matcher/errors`: failures that a call returns as the error of
//! a `Result`.

use crate::helpers::{failure_of, opaque, passes};
use dokimi_assert::{Recorder, Stop, Value, check, soft};
use std::error::Error;
use std::fmt;

/// The sentinel of a missing item, with the item's name.
#[derive(Debug, PartialEq)]
struct Missing(&'static str);

impl fmt::Display for Missing {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} is missing", self.0)
    }
}

impl Error for Missing {}

/// An error that wraps the error that caused it.
#[derive(Debug)]
struct Wrapped(Box<dyn Error + 'static>);

impl fmt::Display for Wrapped {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "the lookup failed: {}", self.0)
    }
}

impl Error for Wrapped {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.0.as_ref())
    }
}

/// An error whose source is its only field, at the address of the error.
#[derive(Debug)]
struct Newtype(Missing);

impl fmt::Display for Newtype {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "the request failed: {}", self.0)
    }
}

impl Error for Newtype {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.0)
    }
}

/// An error that is its own source.
#[derive(Debug)]
struct Looped;

impl fmt::Display for Looped {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a loop")
    }
}

impl Error for Looped {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self)
    }
}

fn wrapped() -> Wrapped {
    Wrapped(Box::new(Missing("the user")))
}

mod no_error {
    use super::*;

    #[test]
    fn passes_ok() {
        passes(|t| check::no_error(t, &Ok::<i32, Missing>(1), "c"));
    }

    #[test]
    fn states_the_error_as_its_debug_text() {
        let failure = failure_of(|t| check::no_error(t, &Err::<(), _>(Missing("x")), "c"));
        assert_eq!(failure.assertion(), Some("err-absent"));
        assert_eq!(failure.detail(), [("got", opaque("Missing(\"x\")"))]);
    }
}

mod has_error {
    use super::*;

    #[test]
    fn passes_an_error() {
        passes(|t| check::has_error(t, &Err::<(), _>(Missing("x")), "c"));
    }

    #[test]
    fn states_no_detail_for_ok() {
        let failure = failure_of(|t| soft::has_error(t, &Ok::<i32, Missing>(1), "c"));
        assert_eq!(failure.assertion(), Some("err-present"));
        assert_eq!(failure.detail(), []);
    }
}

mod error_is {
    use super::*;

    #[test]
    fn passes_an_error_that_wraps_the_target() {
        passes(|t| check::error_is(t, &Wrapped(Box::new(wrapped())), &Missing("the user"), "c"));
    }

    #[test]
    fn passes_a_target_at_the_address_of_its_wrapper() {
        passes(|t| check::error_is(t, &Newtype(Missing("the page")), &Missing("the page"), "c"));
    }

    #[test]
    fn states_the_target_with_the_error() {
        let failure = failure_of(|t| check::error_is(t, &Missing("a"), &Missing("b"), "c"));
        assert_eq!(failure.assertion(), Some("err-is"));
        assert_eq!(
            failure.detail(),
            [
                ("want", opaque("Missing(\"b\")")),
                ("got", opaque("Missing(\"a\")"))
            ]
        );
    }

    #[test]
    fn ends_the_walk_of_a_chain_that_loops() {
        let failure = failure_of(|t| check::error_is(t, &Looped, &Missing("the user"), "c"));
        assert_eq!(failure.got(), Some(&opaque("Looped")));
    }

    #[test]
    fn reads_at_most_100_links_of_a_chain() {
        let chain = |links: usize| {
            let mut error: Box<dyn Error> = Box::new(Missing("the root"));
            for _ in 1..links {
                error = Box::new(Wrapped(error));
            }
            error
        };
        passes(|t| check::error_is(t, chain(100).as_ref(), &Missing("the root"), "c"));
        let failure =
            failure_of(|t| check::error_is(t, chain(101).as_ref(), &Missing("the root"), "c"));
        assert_eq!(failure.assertion(), Some("err-is"));
    }
}

mod error_is_not {
    use super::*;

    #[test]
    fn passes_an_error_without_the_target() {
        passes(|t| check::error_is_not(t, &wrapped(), &Missing("the group"), "c"));
    }

    #[test]
    fn states_the_error_that_wraps_the_target() {
        let failure = failure_of(|t| soft::error_is_not(t, &wrapped(), &Missing("the user"), "c"));
        assert_eq!(failure.assertion(), Some("err-is-not"));
        assert_eq!(
            failure.detail(),
            [("got", opaque(&format!("{:?}", wrapped())))]
        );
    }
}

mod error_as {
    use super::*;

    #[test]
    fn returns_the_first_link_of_the_type() {
        let error = wrapped();
        let seat = Recorder::new();
        let found = check::error_as::<Missing>(&seat, &error, "c");
        assert_eq!(found, Some(&Missing("the user")));
        assert!(!seat.failed());
    }

    #[test]
    fn states_the_name_of_the_type_that_no_link_has() {
        let failure = failure_of(|t| assert_eq!(check::error_as::<Stop>(t, &Looped, "c"), None));
        assert_eq!(failure.assertion(), Some("err-as"));
        assert_eq!(
            failure.detail(),
            [
                ("want", Value::from(std::any::type_name::<Stop>())),
                ("got", opaque("Looped"))
            ]
        );
    }
}
