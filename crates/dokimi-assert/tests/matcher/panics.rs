// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `matcher/panics`: calls that panic.

use crate::helpers::{Scratch, failure_of, in_child, passes};
use dokimi_assert::__private::{message, quietly};
use dokimi_assert::{Recorder, Value, check, soft};
use std::panic::panic_any;

mod panics {
    use super::*;

    #[test]
    fn returns_the_message_of_the_panic() {
        let seat = Recorder::new();
        let raised = check::panics(&seat, || panic!("stopped at {}", 3), "c");
        assert_eq!(raised.as_deref(), Some("stopped at 3"));
        assert!(!seat.failed());
    }

    #[test]
    fn returns_a_static_message() {
        let seat = Recorder::new();
        assert_eq!(
            check::panics(&seat, || panic_any("static"), "c").as_deref(),
            Some("static")
        );
    }

    #[test]
    fn names_a_payload_that_is_no_text() {
        let seat = Recorder::new();
        let raised = check::panics(&seat, || panic_any(42_u8), "c");
        assert_eq!(
            raised.as_deref(),
            Some("a panic with a payload that is no text")
        );
    }

    #[test]
    fn returns_the_message_of_a_panic_inside_another_expected_panic() {
        let seat = Recorder::new();
        let outer = check::panics(
            &seat,
            || {
                let inner = check::panics(&seat, || panic!("inner"), "c");
                panic!("outer after {inner:?}");
            },
            "c",
        );
        assert_eq!(outer.as_deref(), Some("outer after Some(\"inner\")"));
    }

    #[test]
    fn states_no_detail_for_a_body_that_returns() {
        let failure = failure_of(|t| assert_eq!(check::panics(t, || {}, "c"), None));
        assert_eq!(failure.assertion(), Some("throws"));
        assert_eq!(failure.detail(), []);
    }
}

mod does_not_panic {
    use super::*;

    #[test]
    fn passes_a_body_that_returns() {
        passes(|t| check::does_not_panic(t, || {}, "c"));
    }

    #[test]
    fn states_the_message_of_the_panic() {
        let failure = failure_of(|t| soft::does_not_panic(t, || panic!("boom"), "c"));
        assert_eq!(failure.assertion(), Some("not-throws"));
        assert_eq!(failure.detail(), [("got", Value::from("boom"))]);
    }
}

mod quietly {
    use super::*;

    #[test]
    fn catches_a_panic_on_a_thread_that_unwinds() {
        let dir = Scratch::new();
        let output = in_child(
            "matcher::panics::child::catches_a_panic_in_a_drop_that_unwinds",
            &dir.join(""),
            &[],
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stdout.contains("caught: the cleanup broke\n"),
            "{stdout}{stderr}"
        );
    }
}

mod child {
    use super::*;

    /// A value whose drop calls `quietly` with a body that panics.
    struct Releasing;

    impl Drop for Releasing {
        fn drop(&mut self) {
            let Err(raised) = quietly(|| panic!("the cleanup broke"));
            println!("caught: {}", message(&*raised));
        }
    }

    #[test]
    #[ignore = "a spec of quietly runs it in a child process, where no test installed the panic hook"]
    fn catches_a_panic_in_a_drop_that_unwinds() {
        let _releasing = Releasing;
        panic!("the test failed first");
    }
}
