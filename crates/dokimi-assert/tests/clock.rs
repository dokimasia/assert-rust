// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `clock`, the clocks that the timed assertions read.

use dokimi_assert::{Clock, Controlled, Recorder, Seat, System};
use std::sync::Arc;
use std::time::Duration;

mod controlled {
    use super::*;

    #[test]
    fn starts_at_zero() {
        assert_eq!(Controlled::new().now(), Duration::ZERO);
    }

    #[test]
    fn moves_by_the_sum_of_its_advances() {
        let clock = Controlled::new();
        clock.advance(Duration::from_millis(5));
        clock.advance(Duration::from_millis(7));
        assert_eq!(clock.now(), Duration::from_millis(12));
    }

    #[test]
    fn returns_at_once_from_a_sleep_of_zero() {
        Controlled::new().sleep(Duration::ZERO);
    }

    #[test]
    fn wakes_a_sleeper_when_another_thread_advances_past_its_duration() {
        let clock = Arc::new(Controlled::new());
        let sleeper = Arc::clone(&clock);
        let woke = std::thread::spawn(move || {
            sleeper.sleep(Duration::from_secs(10));
            sleeper.now()
        });
        // The sleeper sleeps until 10 seconds past the moment it starts, so
        // the advances continue until it wakes.
        while !woke.is_finished() {
            clock.advance(Duration::from_secs(1));
            std::thread::yield_now();
        }
        let at = woke.join().expect("the sleeper returns");
        assert!(at >= Duration::from_secs(10), "{at:?}");
    }
}

mod system {
    use super::*;

    #[test]
    fn moves_forward() {
        let clock = System::new();
        let first = clock.now();
        clock.sleep(Duration::from_millis(1));
        assert!(clock.now() > first);
    }

    #[test]
    fn ignores_an_advance() {
        let clock = System::default();
        let before = clock.now();
        clock.advance(Duration::from_secs(3600));
        assert!(clock.now() < before + Duration::from_secs(3600));
    }

    #[test]
    fn is_the_clock_of_a_seat_that_supplies_none() {
        let seat = Recorder::new();
        let first = seat.clock().now();
        seat.clock().sleep(Duration::from_millis(1));
        assert!(seat.clock().now() > first);
    }
}
