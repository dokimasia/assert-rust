// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `prop::engine::execution`: one call of a body, how it ended,
//! and its walk into the case tree.

use crate::helpers::{Register, records};
use dokimi_assert::prop::{self, Case};
use dokimi_assert::stateful::{self, Action, Machine};
use dokimi_assert::{Recorder, Value, check, soft};
use std::collections::HashSet;
use std::sync::Mutex;
use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};

/// Returns the machine of a register that `subject` holds.
fn register(subject: &AtomicI64) -> Machine<'_, i64> {
    Machine::new([
        Action::with_input(
            "write",
            |c: &Case<'_>, _: &i64| c.draw(&prop::integer(0..=9_i64), "v"),
            async move |c: &Case<'_>, client: usize, value: i64| {
                let call = c.history().invoke(client, "write", [value.into()], []);
                subject.store(value, Ordering::SeqCst);
                call.ok(Value::Null);
            },
        ),
        Action::new("read", async move |c: &Case<'_>, client: usize| {
            let call = c.history().invoke(client, "read", [], []);
            call.ok(subject.load(Ordering::SeqCst));
        }),
    ])
    .spec(Register)
}

mod execute {
    use super::*;

    #[test]
    fn ends_a_repeated_case_at_its_first_later_run_that_fails() {
        let seen = Mutex::new(HashSet::new());
        let seat = Recorder::new();
        prop::for_all(
            &seat,
            "a case runs once",
            prop::hermetic().seed(1).cases(30).shrink(0),
            |c| {
                let tag = c.draw(&prop::integer(0..=1_000_000_000_u64), "tag");
                let subject = AtomicI64::new(0);
                stateful::steps(c, &register(&subject), stateful::clients(2).repeat(3));
                let first = seen.lock().expect("no case panicked").insert(tag);
                check::is_true(c, first, "a case runs once");
            },
        );
        let detail = &records(&seat)[0]["detail"];
        assert_eq!(detail["outcome"], "counterexample", "{detail}");
    }

    #[test]
    fn ends_a_repeated_case_at_its_first_later_run_that_skips() {
        let seen = Mutex::new(HashSet::new());
        let seat = Recorder::new();
        prop::for_all(
            &seat,
            "a case runs once",
            prop::hermetic().seed(1).cases(30),
            |c| {
                let tag = c.draw(&prop::integer(0..=1_000_000_000_u64), "tag");
                let subject = AtomicI64::new(0);
                stateful::steps(c, &register(&subject), stateful::clients(2).repeat(3));
                if !seen.lock().expect("no case panicked").insert(tag) {
                    c.skip("a later run skips");
                }
            },
        );
        let detail = &records(&seat)[0]["detail"];
        assert_eq!(detail["outcome"], "skipped", "{detail}");
        assert_eq!(detail["reason"], "a later run skips", "{detail}");
    }

    #[test]
    fn keeps_a_failure_that_a_skip_follows() {
        let seat = Recorder::new();
        prop::for_all(
            &seat,
            "a failure outweighs a skip",
            prop::hermetic().seed(1).cases(10),
            |c| {
                let _ = c.draw(&prop::boolean(), "flag");
                soft::is_true(c, false, "the flag is set");
                c.skip("the case skips after its failure");
            },
        );
        let detail = &records(&seat)[0]["detail"];
        assert_eq!(detail["outcome"], "counterexample", "{detail}");
        assert_eq!(detail["reason"], serde_json::Value::Null, "{detail}");
    }
}

mod entered {
    use super::*;

    #[test]
    fn walks_a_case_that_skips_into_the_tree_without_a_leaf() {
        let calls = AtomicUsize::new(0);
        let seat = Recorder::new();
        prop::for_all(
            &seat,
            "a later case skips",
            prop::hermetic().seed(1).cases(10),
            |c| {
                let _ = c.draw(&prop::integer(0..=0_u8), "n");
                if calls.fetch_add(1, Ordering::SeqCst) > 0 {
                    c.skip("a later case skips");
                }
                let _ = c.draw(&prop::boolean(), "flag");
            },
        );
        let detail = &records(&seat)[0]["detail"];
        assert_eq!(detail["outcome"], "skipped", "{detail}");
    }
}
