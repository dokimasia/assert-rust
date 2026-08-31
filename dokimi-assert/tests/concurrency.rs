//! The seats, driven from more than one thread.
//!
//! A test holds one seat and hands it to every assertion in the body, and several
//! of those run the subject somewhere else: one retries a body, one watches for
//! work that outlives its scope, one gives a subject a handle and waits. A seat
//! that lost a failure because two arrived at once would report the wrong answer
//! and no test would see why.

use dokimi_assert::seat::{Recorder, Seat};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

/// How many threads report at once.
const WRITERS: usize = 8;

/// How many failures each of them reports.
const EACH: usize = 2_000;

/// Run body on every writer thread, starting them together so they contend.
fn in_parallel<F>(body: F)
where
    F: Fn() + Send + Sync + 'static,
{
    let held = Arc::new(body);
    let go = Arc::new(AtomicBool::new(false));

    let workers: Vec<_> = (0..WRITERS)
        .map(|_| {
            let run = Arc::clone(&held);
            let start = Arc::clone(&go);
            thread::spawn(move || {
                while !start.load(Ordering::Acquire) {
                    std::hint::spin_loop();
                }
                run();
            })
        })
        .collect();

    go.store(true, Ordering::Release);
    for worker in workers {
        worker.join().expect("a writer finishes");
    }
}

#[test]
fn a_recorder_keeps_every_failure_reported_from_many_threads() {
    let seat = Arc::new(Recorder::new());
    let reporting = Arc::clone(&seat);

    in_parallel(move || {
        for _ in 0..EACH {
            reporting.record("a failure");
        }
    });

    assert_eq!(seat.messages().len(), WRITERS * EACH);
}

#[test]
fn a_recorder_counts_every_helper_mark_from_many_threads() {
    let seat = Arc::new(Recorder::new());
    let marking = Arc::clone(&seat);

    in_parallel(move || {
        for _ in 0..EACH {
            marking.helper();
        }
    });

    assert_eq!(seat.helper_calls(), WRITERS * EACH);
}
