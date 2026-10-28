// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The specs of `dokimi-assert`, in one test binary.
//!
//! Each source file under `src/` has its spec at the same path under
//! `tests/`, and every spec calls the public API alone. The binary installs
//! the counting allocator, as a test binary that states an allocation
//! ceiling installs it. This file is the spec of `lib.rs`: the public names
//! of the crate, which the definition's naming table states.

#![expect(
    clippy::module_inception,
    reason = "a spec names a module after each export, so the spec of panics.rs has a module panics"
)]

use dokimi_assert::bench::CountingAllocator;

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator::new();

mod alloc;
mod bench;
mod cancel;
mod check;
mod clock;
mod failure;
mod files;
mod golden;
mod helpers;
mod history;
mod json;
mod matcher;
mod pattern;
mod prop;
mod record;
mod seat;
mod soft;
mod stateful;
mod value;
mod verdict;

/// The names that the naming table gives Rust.
///
/// The code of this module names each one: a function or a type by an
/// import, and a member by a call on a value of its type. The build fails
/// when a name is missing or a member changes its signature, and the test
/// fails when the table states a row that the code does not name.
mod naming_table {
    #![expect(
        unused_imports,
        reason = "an import names an item of the table, and nothing uses the item"
    )]

    use dokimi_assert::bench::Contract;
    use dokimi_assert::check::{
        accumulates as _, close_to as _, completes_within as _, contains as _,
        contains_in_order as _, does_not_panic as _, equal as _, error_as as _, error_is as _,
        error_is_not as _, eventually as _, eventually_true as _, fails_after_close as _,
        has_error as _, has_prefix as _, has_stable_order as _, has_suffix as _,
        honours_cancellation as _, honours_deadline as _, in_range as _, is_associative as _,
        is_commutative as _, is_deterministic as _, is_empty as _, is_false as _,
        is_idempotent as _, is_monotonic as _, is_none as _, is_not_empty as _, is_not_pure as _,
        is_permutation as _, is_poisoned as _, is_pure as _, is_some as _, is_total as _,
        is_true as _, length as _, matches as _, max_allocs as _, max_allocs_with_setup as _,
        max_live_bytes as _, no_duplicates as _, no_error as _, none_handle_safe as _,
        not_contains as _, not_equal as _, pairwise as _, panics as _, rejects as _,
        round_trip as _,
    };
    use dokimi_assert::files::{
        self, Entry, Tree, absent as _, bytes as _, contains as _, directory as _, equal as _,
        executable as _, has_content as _, has_mode as _, is_dir as _, is_file as _, link as _,
        links_to as _, read as _, text as _, unchanged as _, workspace as _, write as _,
    };
    use dokimi_assert::golden::{
        Scrubber, matches as _, matches_at as _, matches_json_field as _, matches_tree as _,
        scrub_hashes as _, scrub_json_fields as _, scrub_run_ids as _, scrub_timestamps as _,
        should_update as _,
    };
    use dokimi_assert::history::{
        Call, Event, History, Operation, Outcome, Spec, budget as _, concurrently as _,
        from_intervals as _, has_snapshot_isolation as _, is_linearizable as _,
        is_serializable as _, memo_limit as _, spec_from as _, time_limit as _, workers as _,
    };
    use dokimi_assert::prop::{
        self, Case, Generator, accumulates as _, around as _, boolean as _, bytes as _, cases as _,
        close_to as _, composite as _, contains as _, contains_in_order as _, dict as _,
        does_not_panic as _, draws as _, duration as _, equal as _, error_as as _, error_is as _,
        error_is_not as _, example as _, examples as _, explain as _, float as _, for_all as _,
        fuzz as _, has_error as _, has_prefix as _, has_suffix as _, hermetic as _,
        honours_cancellation as _, honours_deadline as _, in_range as _, integer as _,
        is_associative as _, is_commutative as _, is_deterministic as _, is_empty as _,
        is_false as _, is_idempotent as _, is_none as _, is_not_empty as _, is_not_pure as _,
        is_permutation as _, is_pure as _, is_some as _, is_true as _, just as _, length as _,
        list as _, matches as _, max_allocs as _, max_allocs_with_setup as _, max_choices as _,
        no_error as _, none_handle_safe as _, not_contains as _, not_equal as _, of as _,
        of_shape as _, one_of as _, optional as _, pairwise as _, panics as _, permutation as _,
        recursive as _, register as _, register_values as _, register_variants as _, replay as _,
        require as _, round_trip as _, sampled_from as _, seed as _, shape_of as _, shrink as _,
        shrink_time as _, store as _, string as _, string_matching as _, using as _, values as _,
        workers as _,
    };
    use dokimi_assert::soft::{
        accumulates as _, close_to as _, completes_within as _, contains as _,
        contains_in_order as _, does_not_panic as _, equal as _, error_as as _, error_is as _,
        error_is_not as _, eventually as _, eventually_true as _, fails_after_close as _,
        has_error as _, has_prefix as _, has_stable_order as _, has_suffix as _,
        honours_cancellation as _, honours_deadline as _, in_range as _, is_associative as _,
        is_commutative as _, is_deterministic as _, is_empty as _, is_false as _,
        is_idempotent as _, is_monotonic as _, is_none as _, is_not_empty as _, is_not_pure as _,
        is_permutation as _, is_poisoned as _, is_pure as _, is_some as _, is_total as _,
        is_true as _, length as _, matches as _, max_allocs as _, max_allocs_with_setup as _,
        max_live_bytes as _, no_duplicates as _, no_error as _, none_handle_safe as _,
        not_contains as _, not_equal as _, pairwise as _, panics as _, round_trip as _,
    };
    use dokimi_assert::stateful::{
        self, Action, Machine, Scheduler, clients as _, concurrent as _, max as _, mean as _,
        pct as _, repeat as _, steps as _, swarm as _, tasks as _, uniform as _,
    };
    use dokimi_assert::{
        Clock, Collector, Controlled, Failure, Recorder, Seat, Standard, System, Value, Where,
        allocs_checked as _, fault as _,
    };
    use std::sync::Arc;
    use std::time::Duration;

    /// The rows of the table that another crate of the workspace declares:
    /// `dokimi-assert-tokio` declares `no_task_leaks`, and its spec calls it.
    const ELSEWHERE: [&str; 1] = ["no-task-leaks"];

    /// A spec whose state is a count.
    struct Count;

    impl Spec for Count {
        type State = u8;

        fn initial(&self) -> u8 {
            0
        }

        fn next(&self, state: &u8, _: &Operation) -> Vec<u8> {
            vec![*state]
        }
    }

    /// Calls each member that the table names on a value of the type that
    /// declares it. Nothing runs the calls.
    #[expect(
        dead_code,
        reason = "the calls name the members, and nothing runs them"
    )]
    fn members(
        seat: &Recorder,
        collector: &Collector,
        clock: &Controlled,
        failure: &Failure,
        case: &Case<'_>,
    ) {
        seat.helper();
        seat.fail("c");
        seat.record("c");
        seat.report(failure, true);
        seat.report_fault(Arc::new(std::io::Error::other("c")), true);
        // The call of an assertion is `dokimi_assert::Call`, and `Call` here
        // is the call of a history.
        seat.observe_call(&dokimi_assert::Call {
            assertion: "true",
            contract: "c",
        });
        let _ = (seat.clock(), seat.cancellation());
        let _ = (
            seat.failed(),
            seat.failures(),
            seat.faults(),
            seat.helper_calls(),
            seat.message(),
            seat.messages(),
            seat.records(),
        );
        let _ = collector.collected();
        collector.flush();
        let _ = clock.now();
        clock.sleep(Duration::ZERO);
        clock.advance(Duration::ZERO);
        let _ = (
            failure.assertion(),
            failure.case_failure(),
            failure.contract(),
            failure.detail(),
        );
        let _ = (failure.want(), failure.got());
        // The loop of a contract is `run`.
        let contract = Contract::new(seat, "c")
            .warmup(1)
            .max_allocs(0)
            .max_bytes(0);
        let contract = contract
            .max_latency(Duration::ZERO)
            .max_mean(Duration::ZERO)
            .run(1, || {});
        contract
            .measuring(1, || 0_u8, |_| {})
            .run_parallel(1, 1, || {})
            .check();
        let _ = files::directory().with_mode(0o700);
        let _ = prop::integer(0..=1_u8)
            .map(u16::from)
            .filter(|n| *n > 0)
            .bind(prop::just);
        case.assume(true);
        case.classify("c");
        case.note("c");
        case.observe(0);
        case.target("c", 0.0);
        case.cleanup(|| {});
        let _ = (
            case.draw(&prop::boolean(), "flag"),
            case.rand(),
            case.history(),
            case.cancellation(),
        );
        let history = History::new();
        history.invoke(0, "put", [Value::Null], []).ok(Value::Null);
        history.invoke(0, "put", [], []).fail("c");
        history.invoke(0, "put", [], []).unknown("c");
        let _: Vec<Event> = history.events();
        let operation: Option<&Operation> = None;
        let _ = operation.map(|op| {
            (
                op.name(),
                op.args(),
                op.known(),
                op.output(),
                op.returned(Value::Null),
            )
        });
        let _ = operation.map(|op| (Count.initial(), Count.next(&0, op), Count.equal(&0, &0)));
        // An action takes its run in `new`, and its input in `with_input`.
        let put = Action::new("put", async |_, _| {})
            .weight(2)
            .drain()
            .enabled(|_: &u8| true);
        let _ = put.name();
        let get = Action::with_input("get", |_, _: &u8| 0_u8, async |_, _, _| {});
        // A machine takes its actions in `new`.
        let _ = Machine::new([put, get])
            .spec(Count)
            .invariant(|_, _| {})
            .settle(|_, _| {});
        let scheduler = Scheduler::new(case, stateful::uniform());
        scheduler.spawn(async |_| {});
        drop(scheduler.yield_now());
        scheduler.run();
        seat.skip("c");
        // A skip ends the case, so it is the last call.
        case.skip("c");
    }

    /// Returns `true` when `source` contains `name` as a whole identifier.
    fn names(source: &str, name: &str) -> bool {
        source.match_indices(name).any(|(at, _)| {
            let before = source[..at].chars().next_back();
            let after = source[at + name.len()..].chars().next();
            let outside = |c: Option<char>| !c.is_some_and(|c| c.is_alphanumeric() || c == '_');
            outside(before) && outside(after)
        })
    }

    #[test]
    fn names_every_row_of_the_table_in_code() {
        let table: serde_json::Value = serde_json::from_str(include_str!(
            "../../dokimi-assert-conformance/spec/naming.json"
        ))
        .expect("the vendored naming table is JSON");
        let overlay: serde_json::Value = serde_json::from_str(include_str!(
            "../../dokimi-assert-conformance/overlays/rust.json"
        ))
        .expect("the vendored overlay is JSON");
        let declined = |section: &str| -> Vec<String> {
            overlay[section]
                .as_array()
                .map(|rows| {
                    rows.iter()
                        .filter_map(|row| row["id"].as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default()
        };
        let source = include_str!("main.rs");
        let mut rows: Vec<(String, &serde_json::Value)> = Vec::new();
        for (id, row) in table["names"]
            .as_object()
            .expect("the table names the assertions")
        {
            rows.push((id.clone(), row));
        }
        for section in ["types", "members", "helpers"] {
            for (id, row) in table["surface"][section]
                .as_object()
                .expect("the table states each section")
            {
                rows.push((id.clone(), row));
            }
        }
        assert!(rows.len() > 270, "the table states {} rows", rows.len());
        let mut missing = Vec::new();
        for (id, row) in rows {
            if ELSEWHERE.contains(&id.as_str()) {
                continue;
            }
            match row["rust"].as_str() {
                Some(name) => {
                    let leaf = name.rsplit("::").next().unwrap_or(name);
                    if !names(source, leaf) {
                        missing.push(format!("{id}: {name}"));
                    }
                }
                None => assert!(
                    declined("surface").contains(&id),
                    "{id} has no Rust name, and the overlay does not decline it"
                ),
            }
        }
        assert_eq!(
            missing,
            [] as [String; 0],
            "the code names no item for these rows"
        );
        for (id, row) in table["relaxations"]
            .as_object()
            .expect("the table states the relaxations")
        {
            assert_eq!(
                row["rust"],
                serde_json::Value::Null,
                "{id} has a Rust name, and no code names it"
            );
            assert!(
                declined("relaxations").contains(id),
                "{id} has no Rust name, and the overlay does not decline it"
            );
        }
    }

    #[test]
    fn finds_a_name_as_a_whole_identifier_alone() {
        assert!(names("let _ = run(1);", "run"));
        assert!(!names("let _ = run_parallel(1);", "run"));
        assert!(!names("let _ = rerun(1);", "run"));
    }
}
