// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `prop::detail`: the detail of a property's record and its
//! sentence.

use crate::helpers::records;
use dokimi_assert::prop::{self, Case};
use dokimi_assert::stateful::{self, Action, Machine};
use dokimi_assert::{Recorder, Seat, Value, check};
use serde_json::json;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// Returns the recorder of a run of `body` with the seed 1 and `options`.
fn run(options: prop::Options, body: impl Fn(&Case<'_>) + Send + Sync) -> Recorder {
    let seat = Recorder::new();
    prop::for_all(
        &seat,
        "the property holds",
        options.hermetic().seed(1).store(""),
        body,
    );
    seat
}

/// Returns the detail of the first call record of `seat`.
fn detail(seat: &Recorder) -> serde_json::Value {
    records(seat)[0]["detail"].clone()
}

/// Returns the recorder of a run whose first call of the body takes
/// `first`, and whose every later call takes `later`. The first call is the
/// simplest case, which fails, so the replay of the failure takes `later`.
fn diverging(
    first: impl Fn(&Case<'_>) + Send + Sync,
    later: impl Fn(&Case<'_>) + Send + Sync,
) -> Recorder {
    let calls = AtomicUsize::new(0);
    run(prop::Options::default(), move |c| {
        if calls.fetch_add(1, Ordering::SeqCst) == 0 {
            first(c);
        } else {
            later(c);
        }
    })
}

mod reason {
    use super::*;

    #[test]
    fn states_null_for_a_run_that_does_not_skip() {
        let seat = run(prop::Options::default(), |c| {
            let n = c.draw(&prop::integer(0..=9_u8), "n");
            check::is_true(c, n < 3, "n stays below 3");
        });
        assert_eq!(detail(&seat)["reason"], serde_json::Value::Null);
        let fields = seat.failures()[0].detail().to_vec();
        assert_eq!(fields.len(), 11, "{fields:?}");
        assert_eq!(fields[10], ("reason", Value::Null), "{fields:?}");
    }
}

mod counterexample {
    use super::*;

    #[test]
    fn states_the_draws_the_failure_and_the_replay_token() {
        let seat = run(prop::Options::default(), |c| {
            let n = c.draw(&prop::integer(0..=9_u8), "n");
            check::is_true(c, n < 3, "n stays below 3");
        });
        let sentence = seat.message();
        assert!(
            sentence.starts_with("the property holds: counterexample after "),
            "{sentence}"
        );
        assert!(sentence.contains("\n  n: 3, 2 passes"), "{sentence}");
        assert!(
            sentence.contains("\nfailure of true at detail.rs:"),
            "{sentence}"
        );
        assert!(
            sentence.contains("\nreplay: prop::replay(\"prop1:"),
            "{sentence}"
        );
        let stated = detail(&seat);
        assert_eq!(stated["failure"]["assertion"], "true");
        assert_eq!(stated["failure"]["where"]["file"], "detail.rs");
    }

    #[test]
    fn states_each_other_failure_with_its_replay_token() {
        let both = r#"[{"label": "a", "value": {"type": "int", "value": 7}}, {"label": "b", "value": {"type": "int", "value": 7}}]"#;
        let seat = run(prop::draws(both), |c| {
            let a = c.draw(&prop::integer(0..=9_u8), "a");
            let b = c.draw(&prop::integer(0..=9_u8), "b");
            if a < 5 && b >= 5 {
                c.fail("b fails");
            }
            check::is_true(c, a < 5, "a stays below 5");
        });
        let sentence = seat.message();
        assert!(
            sentence.contains("\nother failure at detail.rs:"),
            "{sentence}"
        );
        assert_eq!(
            detail(&seat)["others"][0]["failure"]["assertion"],
            serde_json::Value::Null
        );
    }

    #[test]
    fn states_a_failure_without_a_record_by_its_identity() {
        let seat = run(prop::Options::default(), |c| {
            let _ = c.draw(&prop::boolean(), "flag");
            c.fail_as(r#"["lost",null,null,null,null]"#, true);
        });
        assert_eq!(
            detail(&seat)["failure"],
            json!(r#"["lost",null,null,null,null]"#)
        );
        assert!(
            seat.message().contains("\nfailure: lost"),
            "{}",
            seat.message()
        );
    }

    #[test]
    fn states_the_steps_of_a_machine() {
        let seat = run(
            prop::draws(r#"[{"step": "put", "client": 1}, {"step": "flush", "drain": true}]"#),
            |c| {
                let (put, flushed) = (AtomicUsize::new(0), AtomicBool::new(false));
                let machine = Machine::<i64>::new([
                    Action::new("put", async |_: &Case<'_>, client: usize| {
                        put.store(client, Ordering::SeqCst);
                    }),
                    Action::new("flush", async |_: &Case<'_>, _: usize| {
                        flushed.store(true, Ordering::SeqCst);
                    })
                    .enabled(|_| put.load(Ordering::SeqCst) > 0 && !flushed.load(Ordering::SeqCst))
                    .drain(),
                ])
                .settle(|c: &Case<'_>, _: &i64| {
                    check::is_false(c, flushed.load(Ordering::SeqCst), "nothing was flushed");
                });
                stateful::steps(c, &machine, stateful::clients(2));
            },
        );
        let sentence = seat.message();
        assert!(sentence.contains("\n  step put on client 1"), "{sentence}");
        assert!(
            sentence.contains("\n  step flush in the drain"),
            "{sentence}"
        );
        let entries = detail(&seat)["counterexample"].clone();
        assert_eq!(entries[0], json!({"step": "put", "client": 1}));
        assert_eq!(entries[1], json!({"step": "flush", "drain": true}));
    }
}

mod divergence {
    use super::*;

    #[test]
    fn states_the_integer_bounds_of_a_request_that_differs() {
        let seat = diverging(
            |c| {
                let _ = c.draw(&prop::integer(0..=9_u64), "n");
                c.fail("the case fails");
            },
            |c| {
                let _ = c.draw(&prop::integer(0..=(1_u64 << 60)), "n");
                c.fail("the case fails");
            },
        );
        let stated = detail(&seat)["divergence"].clone();
        assert_eq!(stated["what"], "request");
        assert_eq!(
            stated["recorded"],
            json!({"kind": "integer", "min": 0, "max": 9})
        );
        assert_eq!(
            stated["replayed"],
            json!({"kind": "integer", "min": 0, "max": "1152921504606846976"})
        );
        assert_eq!(stated["label"], "n");
        assert!(
            seat.message()
                .contains("\ndivergence: the request at 0 (in the draw \"n\"), recorded ")
        );
    }

    #[test]
    fn states_the_float_and_the_sequence_bounds_of_a_request_that_differs() {
        let seat = diverging(
            |c| {
                let _ = c.draw(&prop::float(0.0..=1.0_f64), "x");
                c.fail("the case fails");
            },
            |c| {
                let _ = c.draw(&prop::float(f64::NEG_INFINITY..=f64::INFINITY), "x");
                c.fail("the case fails");
            },
        );
        let stated = detail(&seat)["divergence"].clone();
        assert_eq!(stated["recorded"]["min"], 0.0);
        assert_eq!(stated["replayed"]["min"], "-Inf");
        assert_eq!(stated["replayed"]["max"], "Inf");
        let seat = diverging(
            |c| {
                let _ = c.draw(&prop::string(..=3), "s");
                c.fail("the case fails");
            },
            |c| {
                let _ = c.draw(&prop::string(..), "s");
                c.fail("the case fails");
            },
        );
        let stated = detail(&seat)["divergence"].clone();
        assert_eq!(stated["recorded"]["max_size"], 3);
        assert_eq!(stated["replayed"]["max_size"], serde_json::Value::Null);
    }

    #[test]
    fn states_a_request_that_the_replay_lacks() {
        let seat = diverging(
            |c| {
                let _ = c.draw(&prop::boolean(), "flag");
                c.fail("the case fails");
            },
            |c| c.fail("the case fails"),
        );
        assert_eq!(
            detail(&seat)["divergence"]["replayed"],
            serde_json::Value::Null
        );
        assert!(
            seat.message().contains("replayed no request"),
            "{}",
            seat.message()
        );
    }

    #[test]
    fn states_a_fingerprint_that_differs() {
        let seat = diverging(
            |c| {
                c.observe(u64::MAX);
                c.fail("the case fails");
            },
            |c| {
                c.observe(1);
                c.fail("the case fails");
            },
        );
        let stated = detail(&seat)["divergence"].clone();
        assert_eq!(stated["what"], "fingerprint");
        assert_eq!(stated["recorded"], json!("18446744073709551615"));
        assert_eq!(stated["replayed"], json!(1));
        let seat = diverging(
            |c| {
                c.observe(1);
                c.fail("the case fails");
            },
            |c| c.fail("the case fails"),
        );
        assert!(
            seat.message().contains("replayed no fingerprint"),
            "{}",
            seat.message()
        );
    }

    #[test]
    fn states_a_replay_that_passed_or_failed_another_way() {
        let seat = diverging(|c| c.fail("the case fails"), |_| {});
        assert_eq!(detail(&seat)["divergence"]["what"], "verdict");
        assert!(
            seat.message().contains("replayed no failure"),
            "{}",
            seat.message()
        );
        let seat = diverging(|_| panic!("the body fails"), |c| c.fail("the case fails"));
        let stated = detail(&seat)["divergence"].clone();
        assert!(
            stated["recorded"]
                .as_str()
                .is_some_and(|text| text.starts_with("a panic at ")),
            "{stated}"
        );
        assert!(
            stated["replayed"]
                .as_str()
                .is_some_and(|text| text.starts_with("a message at detail.rs:")),
            "{stated}"
        );
        let seat = diverging(
            |c| check::is_true(c, false, "the flag is set"),
            |c| {
                c.fail_with(
                    "lost-write",
                    "the store keeps every put",
                    vec![("key", Value::Int(1))],
                );
            },
        );
        let stated = detail(&seat)["divergence"].clone();
        assert!(
            stated["recorded"]
                .as_str()
                .is_some_and(|text| text.starts_with("true at detail.rs:")),
            "{stated}"
        );
        assert_eq!(
            stated["replayed"],
            json!("lost-write of \"the store keeps every put\"")
        );
    }
}
