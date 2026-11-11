// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `stateful::scheduler`, the task scheduler.

use dokimi_assert::prop::Case;
use dokimi_assert::stateful::{self, Action, Machine, Scheduler, Strategy};
use dokimi_assert::{Clock, Recorder, prop};
use std::collections::BTreeSet;
use std::future::{Future, poll_fn};
use std::pin::pin;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::task::{Context, Poll, Waker};
use std::time::Duration;

/// The instant at which the clock of every scheduler starts, midnight UTC
/// 2000-01-01, as the time since the Unix epoch.
const EPOCH: Duration = Duration::from_secs(946_684_800);

/// Takes a lock whose poison a panicking task left.
fn locked<T>(lock: &Mutex<T>) -> MutexGuard<'_, T> {
    lock.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A flag that one task awaits and another sets, which wakes the waiting
/// task.
#[derive(Default)]
struct Flag {
    state: Mutex<(bool, Option<Waker>)>,
}

impl Flag {
    fn set(&self) {
        let mut state = locked(&self.state);
        state.0 = true;
        if let Some(waker) = state.1.take() {
            waker.wake();
        }
    }

    fn wait(&self) -> impl Future<Output = ()> + Send + '_ {
        poll_fn(|context: &mut Context<'_>| {
            let mut state = locked(&self.state);
            if state.0 {
                return Poll::Ready(());
            }
            state.1 = Some(context.waker().clone());
            Poll::Pending
        })
    }
}

/// Counts its drop.
struct Guard<'a>(&'a AtomicUsize);

impl Drop for Guard<'_> {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

/// Runs `body` as a property of 30 cases with the seed 1 on a recorder, and
/// returns the recorder.
fn run(body: impl Fn(&Case<'_>) + Send + Sync) -> Recorder {
    let seat = Recorder::new();
    prop::for_all(
        &seat,
        "the tasks run",
        prop::hermetic().seed(1).cases(30),
        body,
    );
    seat
}

/// Returns the contract of the failure of the first failing case of a run of
/// `body`.
fn case_failure(body: impl Fn(&Case<'_>) + Send + Sync) -> Option<String> {
    let seat = run(body);
    seat.failures().first().and_then(|failure| {
        failure
            .case_failure()
            .map(|case| case.contract().to_owned())
    })
}

/// Returns the log that `body` writes in the simplest case of a run, whose
/// every release takes the first ready task.
fn simplest_log(body: impl Fn(&Case<'_>, &Mutex<Vec<String>>) + Send + Sync) -> Vec<String> {
    let first = Mutex::new(None);
    let seat = run(|c| {
        let log = Mutex::new(Vec::new());
        body(c, &log);
        locked(&first).get_or_insert_with(|| log.into_inner().unwrap_or_default());
    });
    assert!(!seat.failed(), "{}", seat.message());
    first.into_inner().unwrap_or_default().unwrap_or_default()
}

/// Returns the first entries of the logs of two tasks that each log their
/// name, yield and log it again, under `strategy`, one per case.
fn first_entries(strategy: Strategy) -> BTreeSet<&'static str> {
    let firsts = Mutex::new(BTreeSet::new());
    let seat = run(|c| {
        let log = Mutex::new(Vec::new());
        let scheduler = Scheduler::new(c, strategy);
        for name in ["a", "b", "c"] {
            let log = &log;
            scheduler.spawn(move |s| async move {
                locked(log).push(name);
                s.yield_now().await;
                locked(log).push(name);
            });
        }
        scheduler.run();
        let log = locked(&log);
        assert_eq!(log.len(), 6);
        locked(&firsts).insert(log[0]);
    });
    assert!(!seat.failed(), "{}", seat.message());
    firsts.into_inner().unwrap_or_else(PoisonError::into_inner)
}

mod scheduler {
    use super::*;

    #[test]
    fn releases_the_tasks_in_orders_that_the_case_chooses() {
        assert_eq!(
            first_entries(stateful::uniform()),
            BTreeSet::from(["a", "b", "c"])
        );
    }

    #[test]
    fn releases_the_tasks_by_priority_under_pct() {
        assert_eq!(
            first_entries(stateful::pct(3)),
            BTreeSet::from(["a", "b", "c"])
        );
    }

    #[test]
    fn rejects_a_case_whose_releases_pass_its_cap_on_choices() {
        for strategy in [stateful::uniform(), stateful::pct(2)] {
            let seat = Recorder::new();
            prop::for_all(
                &seat,
                "the tasks run",
                prop::hermetic().seed(1).cases(5).max_choices(1),
                |c| {
                    let scheduler = Scheduler::new(c, strategy);
                    for _ in 0..3 {
                        scheduler.spawn(|s| async move { s.yield_now().await });
                    }
                    scheduler.run();
                },
            );
            let record: serde_json::Value =
                serde_json::from_str(&seat.records()[0]).expect("a call record is JSON");
            assert_eq!(record["detail"]["outcome"], "rejected", "{record}");
        }
    }

    #[test]
    fn continues_a_task_that_another_task_woke() {
        let seat = run(|c| {
            let (flag, log) = (Flag::default(), Mutex::new(Vec::new()));
            let scheduler = Scheduler::new(c, stateful::uniform());
            let (flag, log) = (&flag, &log);
            scheduler.spawn(move |_| async move {
                flag.wait().await;
                locked(log).push("woken");
            });
            scheduler.spawn(move |s| async move {
                s.yield_now().await;
                flag.set();
            });
            scheduler.run();
            assert_eq!(*locked(log), ["woken"]);
        });
        assert!(!seat.failed(), "{}", seat.message());
    }

    #[test]
    fn ends_the_case_with_a_fault_at_a_task_that_no_task_wakes() {
        let seat = run(|c| {
            let scheduler = Scheduler::new(c, stateful::uniform());
            scheduler.spawn(|_| std::future::pending::<()>());
            scheduler.run();
        });
        assert_eq!(
            seat.message(),
            "prop::for_all: stateful: the tasks 0 wait on a future that no task of the scheduler wakes"
        );
    }

    #[test]
    fn raises_the_panic_of_a_task_on_the_thread_of_the_run() {
        let failed = case_failure(|c| {
            let scheduler = Scheduler::new(c, stateful::uniform());
            scheduler.spawn(|_| async { panic!("the task fails") });
            scheduler.run();
        });
        assert_eq!(failed.as_deref(), Some("the task fails"));
    }

    #[test]
    fn panics_when_a_task_runs_its_own_scheduler() {
        let failed = case_failure(|c| {
            let scheduler = Scheduler::new(c, stateful::uniform());
            scheduler.spawn(|s| async move { s.run() });
            scheduler.run();
        });
        assert_eq!(
            failed.as_deref(),
            Some("stateful: a task called run() of its own scheduler")
        );
    }

    #[test]
    fn makes_a_yield_outside_a_turn_ready_at_once() {
        let seat = run(|c| {
            c.draw(&prop::boolean(), "flag");
            let scheduler = Scheduler::new(c, stateful::uniform());
            let mut yielded = pin!(scheduler.yield_now());
            let mut context = Context::from_waker(Waker::noop());
            assert_eq!(yielded.as_mut().poll(&mut context), Poll::Ready(()));
            assert_eq!(
                format!("{scheduler:?}"),
                "Scheduler { strategy: Strategy { depth: 0, max_releases: 1000000 }, .. }"
            );
        });
        assert!(!seat.failed(), "{}", seat.message());
    }

    #[test]
    fn fails_the_case_at_the_limit_while_a_task_is_still_ready() {
        let seat = run(|c| {
            c.draw(&prop::boolean(), "flag");
            let scheduler = Scheduler::new(c, stateful::max_releases(50));
            scheduler.spawn(|s| async move {
                loop {
                    s.yield_now().await;
                }
            });
            scheduler.run();
        });
        let failure = &seat.failures()[0];
        let case = failure.case_failure().expect("the case fails");
        assert_eq!(
            (case.assertion(), case.contract()),
            (
                None,
                "the task scheduler released 50 tasks in one run, and a task is still ready"
            )
        );
    }

    #[test]
    fn ends_a_parked_client_with_its_section() {
        let (resumed, ended) = (AtomicBool::new(false), AtomicUsize::new(0));
        let seat = run(|c| {
            let scheduler = Scheduler::new(c, stateful::uniform());
            let machine =
                Machine::<()>::new([Action::new("park", async |_: &Case<'_>, client: usize| {
                    if client > 0 {
                        let _guard = Guard(&ended);
                        scheduler.wait(|| false).await;
                        resumed.store(true, Ordering::SeqCst);
                    }
                })]);
            stateful::steps(c, &machine, stateful::clients(2).tasks(&scheduler));
        });
        assert!(!seat.failed(), "{}", seat.message());
        assert!(!resumed.load(Ordering::SeqCst));
        assert!(ended.load(Ordering::SeqCst) > 0);
    }

    #[test]
    fn ends_every_task_that_has_not_returned_when_it_drops() {
        let (returned, ended) = (AtomicBool::new(false), AtomicUsize::new(0));
        let seat = run(|c| {
            let (returned, ended) = (&returned, &ended);
            let scheduler = Scheduler::new(c, stateful::uniform());
            scheduler.spawn(move |s| async move {
                let _guard = Guard(ended);
                s.wait(|| false).await;
                returned.store(true, Ordering::SeqCst);
            });
            scheduler.run();
        });
        assert!(!seat.failed(), "{}", seat.message());
        assert!(!returned.load(Ordering::SeqCst));
        assert!(ended.load(Ordering::SeqCst) > 0);
    }
}

mod wait {
    use super::*;

    #[test]
    fn parks_a_task_until_its_condition_is_true_in_a_later_run() {
        let seat = run(|c| {
            let (flag, log) = (AtomicBool::new(false), Mutex::new(Vec::new()));
            let scheduler = Scheduler::new(c, stateful::uniform());
            let (flag, log) = (&flag, &log);
            scheduler.spawn(move |s| async move {
                s.wait(|| flag.load(Ordering::SeqCst)).await;
                locked(log).push("woken");
            });
            scheduler.run();
            assert!(
                locked(log).is_empty(),
                "the run returns while the task waits"
            );
            flag.store(true, Ordering::SeqCst);
            scheduler.run();
            assert_eq!(*locked(log), ["woken"]);
        });
        assert!(!seat.failed(), "{}", seat.message());
    }

    #[test]
    fn makes_the_woken_tasks_ready_in_the_order_of_their_waits() {
        let log = simplest_log(|c, log| {
            let flag = AtomicBool::new(false);
            let scheduler = Scheduler::new(c, stateful::uniform());
            let flag = &flag;
            for name in ["a", "b"] {
                scheduler.spawn(move |s| async move {
                    s.wait(|| flag.load(Ordering::SeqCst)).await;
                    locked(log).push(name.to_owned());
                });
            }
            scheduler.spawn(move |s| async move {
                flag.store(true, Ordering::SeqCst);
                s.yield_now().await;
                locked(log).push("setter".to_owned());
            });
            scheduler.run();
        });
        assert_eq!(log, ["setter", "a", "b"]);
    }

    #[test]
    fn states_whether_a_poll_calls_its_condition() {
        let log = simplest_log(|c, log| {
            let scheduler = Scheduler::new(c, stateful::uniform());
            locked(log).push(format!("{:?}", scheduler.wait(|| true)));
            scheduler.spawn(move |s| async move {
                locked(log).push(format!("{:?}", s.wait(|| true)));
            });
            scheduler.run();
        });
        assert_eq!(
            log,
            ["Wait { pending: false, .. }", "Wait { pending: true, .. }"]
        );
    }

    #[test]
    fn returns_at_once_outside_a_task_for_a_condition_that_is_true() {
        let seat = run(|c| {
            c.draw(&prop::boolean(), "flag");
            let scheduler = Scheduler::new(c, stateful::uniform());
            let mut waited = pin!(scheduler.wait(|| true));
            let mut context = Context::from_waker(Waker::noop());
            assert_eq!(waited.as_mut().poll(&mut context), Poll::Ready(()));
        });
        assert!(!seat.failed(), "{}", seat.message());
    }

    #[test]
    fn panics_outside_a_task_for_a_condition_that_is_false() {
        let failed = case_failure(|c| {
            let scheduler = Scheduler::new(c, stateful::uniform());
            drop(scheduler.wait(|| false));
        });
        assert_eq!(
            failed.as_deref(),
            Some("stateful: a wait outside a task waits for a condition that is false")
        );
    }

    #[test]
    fn fails_the_case_with_the_panic_of_a_condition() {
        let failed = case_failure(|c| {
            let (calls, flag) = (AtomicUsize::new(0), AtomicBool::new(false));
            let scheduler = Scheduler::new(c, stateful::uniform());
            let (calls, flag) = (&calls, &flag);
            scheduler.spawn(move |s| async move {
                s.wait(move || {
                    assert!(
                        calls.fetch_add(1, Ordering::SeqCst) == 0,
                        "the condition fails"
                    );
                    flag.load(Ordering::SeqCst)
                })
                .await;
            });
            scheduler.spawn(|s| async move { s.yield_now().await });
            scheduler.run();
        });
        assert_eq!(failed.as_deref(), Some("the condition fails"));
    }
}

mod clock {
    use super::*;

    #[test]
    fn starts_at_midnight_utc_on_the_first_of_january_2000() {
        let seat = run(|c| {
            c.draw(&prop::boolean(), "flag");
            let scheduler = Scheduler::new(c, stateful::uniform());
            assert_eq!(scheduler.clock().now(), EPOCH);
            scheduler.advance(Duration::from_millis(3));
            assert_eq!(scheduler.clock().now(), EPOCH + Duration::from_millis(3));
        });
        assert!(!seat.failed(), "{}", seat.message());
    }

    #[test]
    fn parks_a_sleep_until_an_advance_passes_its_deadline() {
        let log = simplest_log(|c, log| {
            let scheduler = Scheduler::new(c, stateful::uniform());
            scheduler.spawn(move |s| async move {
                s.clock().sleep(Duration::from_millis(4)).await;
                let now = s.clock().now().saturating_sub(EPOCH);
                locked(log).push(format!("woken at {}", now.as_millis()));
            });
            scheduler.advance(Duration::from_millis(3));
            locked(log).push("advanced 3".to_owned());
            scheduler.advance(Duration::from_millis(1));
        });
        assert_eq!(log, ["advanced 3", "woken at 4"]);
    }

    #[test]
    fn runs_each_task_at_its_own_deadline_within_one_advance() {
        let log = simplest_log(|c, log| {
            let scheduler = Scheduler::new(c, stateful::uniform());
            scheduler.spawn(move |s| async move {
                for _ in 0..3 {
                    s.clock().sleep(Duration::from_millis(4)).await;
                    let now = s.clock().now().saturating_sub(EPOCH);
                    locked(log).push(format!("tick at {}", now.as_millis()));
                }
            });
            scheduler.advance(Duration::from_millis(10));
        });
        assert_eq!(log, ["tick at 4", "tick at 8"]);
    }

    #[test]
    fn wakes_the_sleeps_by_deadline_and_then_in_the_order_of_the_sleeps() {
        let log = simplest_log(|c, log| {
            let scheduler = Scheduler::new(c, stateful::uniform());
            for (name, ms) in [("a", 2), ("b", 1), ("c", 1)] {
                scheduler.spawn(move |s| async move {
                    s.clock().sleep(Duration::from_millis(ms)).await;
                    locked(log).push(name.to_owned());
                });
            }
            scheduler.advance(Duration::from_millis(2));
        });
        assert_eq!(log, ["b", "c", "a"]);
    }

    #[test]
    fn states_the_deadline_of_a_sleep() {
        let log = simplest_log(|c, log| {
            let scheduler = Scheduler::new(c, stateful::uniform());
            locked(log).push(format!("{:?}", scheduler.clock().sleep(Duration::ZERO)));
            scheduler.spawn(move |s| async move {
                let sleep = s.clock().sleep(Duration::from_millis(1));
                locked(log).push(format!("{sleep:?}"));
            });
            scheduler.run();
        });
        assert_eq!(
            log,
            [
                "Sleep { deadline: None, .. }",
                "Sleep { deadline: Some(946684800.001s), .. }"
            ]
        );
    }

    #[test]
    fn returns_at_once_from_a_sleep_of_zero() {
        let seat = run(|c| {
            c.draw(&prop::boolean(), "flag");
            let scheduler = Scheduler::new(c, stateful::uniform());
            let clock = scheduler.clock();
            Clock::sleep(&clock, Duration::ZERO);
            let mut slept = pin!(clock.sleep(Duration::ZERO));
            let mut context = Context::from_waker(Waker::noop());
            assert_eq!(slept.as_mut().poll(&mut context), Poll::Ready(()));
        });
        assert!(!seat.failed(), "{}", seat.message());
    }

    #[test]
    fn panics_outside_a_task_for_a_sleep_of_a_positive_duration() {
        let message = "stateful: a sleep outside a task waits for a clock that only the body moves";
        let failed = case_failure(|c| {
            let scheduler = Scheduler::new(c, stateful::uniform());
            drop(scheduler.clock().sleep(Duration::from_millis(1)));
        });
        assert_eq!(failed.as_deref(), Some(message));
        let failed = case_failure(|c| {
            let scheduler = Scheduler::new(c, stateful::uniform());
            Clock::sleep(&scheduler.clock(), Duration::from_millis(1));
        });
        assert_eq!(failed.as_deref(), Some(message));
    }

    #[test]
    fn panics_for_a_blocking_sleep_inside_a_task() {
        let failed = case_failure(|c| {
            let scheduler = Scheduler::new(c, stateful::uniform());
            scheduler.spawn(|s| async move {
                Clock::sleep(&s.clock(), Duration::from_millis(1));
            });
            scheduler.run();
        });
        assert_eq!(
            failed.as_deref(),
            Some("stateful: a sleep outside a task waits for a clock that only the body moves")
        );
    }

    #[test]
    fn states_the_time_of_its_scheduler() {
        let seat = run(|c| {
            c.draw(&prop::boolean(), "flag");
            let scheduler = Scheduler::new(c, stateful::uniform());
            assert_eq!(
                format!("{:?}", scheduler.clock()),
                "TaskClock { now: 946684800s }"
            );
        });
        assert!(!seat.failed(), "{}", seat.message());
    }
}

mod advance {
    use super::*;

    #[test]
    fn moves_the_clock_past_a_waiting_task_to_the_deadline_of_a_sleep() {
        let log = simplest_log(|c, log| {
            let flag = AtomicBool::new(false);
            let scheduler = Scheduler::new(c, stateful::uniform());
            let flag = &flag;
            scheduler.spawn(move |s| async move {
                s.wait(|| flag.load(Ordering::SeqCst)).await;
                let now = s.clock().now().saturating_sub(EPOCH);
                locked(log).push(format!("woken at {}", now.as_millis()));
            });
            scheduler.spawn(move |s| async move {
                s.clock().sleep(Duration::from_millis(2)).await;
                flag.store(true, Ordering::SeqCst);
            });
            scheduler.advance(Duration::from_millis(5));
        });
        assert_eq!(log, ["woken at 2"]);
    }

    #[test]
    fn releases_the_ready_tasks_as_run_does() {
        let log = simplest_log(|c, log| {
            let scheduler = Scheduler::new(c, stateful::uniform());
            scheduler.spawn(move |_| async move {
                locked(log).push("ran".to_owned());
            });
            scheduler.advance(Duration::ZERO);
        });
        assert_eq!(log, ["ran"]);
    }

    #[test]
    fn panics_when_a_task_advances_its_own_scheduler() {
        let failed = case_failure(|c| {
            let scheduler = Scheduler::new(c, stateful::uniform());
            scheduler.spawn(|s| async move { s.advance(Duration::ZERO) });
            scheduler.run();
        });
        assert_eq!(
            failed.as_deref(),
            Some("stateful: a task called advance() of its own scheduler")
        );
    }

    #[test]
    fn counts_the_releases_of_every_move_of_the_clock() {
        let failed = case_failure(|c| {
            c.draw(&prop::boolean(), "flag");
            let scheduler = Scheduler::new(c, stateful::max_releases(50));
            scheduler.spawn(|s| async move {
                loop {
                    s.clock().sleep(Duration::from_millis(1)).await;
                }
            });
            scheduler.advance(Duration::from_millis(100));
        });
        assert_eq!(
            failed.as_deref(),
            Some("the task scheduler released 50 tasks in one run, and a task is still ready")
        );
    }
}
