//! The spec of `history::options`, the settings of a check of
//! linearizability.

use crate::helpers::{Register, panic_of, read, records, write};
use dokimi_assert::Recorder;
use dokimi_assert::history::{self, History, Options};
use std::time::Duration;

/// Returns a history of two concurrent writes and a read of the second,
/// whose search takes a few steps.
fn concurrent_writes() -> History {
    let h = History::new();
    let first = h.invoke(0, "write", [1.into()], ["x".into()]);
    let second = h.invoke(1, "write", [2.into()], ["x".into()]);
    first.ok(dokimi_assert::Value::Null);
    second.ok(dokimi_assert::Value::Null);
    read(&h, 0, 1);
    h
}

/// Returns the detail of the record of a check of `h` under `options`.
fn detail(h: &History, options: Options) -> serde_json::Value {
    let seat = Recorder::new();
    history::is_linearizable(&seat, h, &Register, "the register is linearizable", options);
    records(&seat)[0]["detail"].clone()
}

mod options {
    use super::*;

    #[test]
    fn states_the_defaults_of_the_definition() {
        let defaults = Options::default();
        assert_eq!(history::budget(10_000_000), defaults);
        assert_eq!(history::memo_limit(1 << 33), defaults);
        assert_eq!(history::time_limit(Duration::ZERO), defaults);
        assert_eq!(history::workers(1), defaults);
    }

    #[test]
    fn chains_one_setting_after_another() {
        let chained = history::budget(5)
            .memo_limit(6)
            .time_limit(Duration::from_secs(7))
            .workers(8);
        assert_ne!(chained, history::budget(5));
        assert_eq!(
            chained,
            history::workers(8)
                .budget(5)
                .memo_limit(6)
                .time_limit(Duration::from_secs(7))
        );
    }
}

mod budget {
    use super::*;

    #[test]
    fn ends_a_search_at_the_steps_limit() {
        let stated = detail(&concurrent_writes(), history::budget(1));
        assert_eq!(stated["outcome"], "undecided");
        assert_eq!(stated["limit"], "steps");
        assert_eq!(stated["steps"], 1);
    }

    #[test]
    fn panics_for_no_step() {
        assert_eq!(
            panic_of(|| _ = history::budget(0)).as_deref(),
            Some("history: budget(0) admits no step")
        );
    }
}

mod memo_limit {
    use super::*;

    #[test]
    fn ends_a_search_at_the_memo_limit() {
        let stated = detail(&concurrent_writes(), history::memo_limit(3));
        assert_eq!(stated["outcome"], "undecided");
        assert_eq!(stated["limit"], "memo");
    }

    #[test]
    fn panics_for_no_bit() {
        assert_eq!(
            panic_of(|| _ = history::memo_limit(0)).as_deref(),
            Some("history: memo_limit(0) admits no configuration")
        );
    }
}

mod time_limit {
    use super::*;

    #[test]
    fn ends_a_search_whose_time_has_passed() {
        let stated = detail(
            &concurrent_writes(),
            history::time_limit(Duration::from_nanos(1)),
        );
        assert_eq!(stated["outcome"], "undecided");
        assert_eq!(stated["limit"], "time");
        assert_eq!(stated["steps"], 0);
    }

    #[test]
    fn lets_a_search_within_its_time_pass() {
        let h = History::new();
        write(&h, 0, 1);
        read(&h, 0, 1);
        let seat = Recorder::new();
        history::is_linearizable(
            &seat,
            &h,
            &Register,
            "c",
            history::time_limit(Duration::from_secs(60)),
        );
        assert!(!seat.failed());
    }
}

mod workers {
    use super::*;

    #[test]
    fn reports_what_one_worker_reports() {
        let h = History::new();
        h.invoke(0, "write", [1.into()], ["a".into()])
            .ok(dokimi_assert::Value::Null);
        h.invoke(0, "write", [1.into()], ["b".into()])
            .ok(dokimi_assert::Value::Null);
        h.invoke(0, "read", [], ["b".into()]).ok(2);
        h.invoke(0, "read", [], ["a".into()]).ok(1);
        let one = detail(&h, history::workers(1));
        assert_eq!(detail(&h, history::workers(4)), one);
        assert_eq!(one["partitions"], 2);
        assert_eq!(one["partition"][0]["value"], "b");
    }

    #[test]
    fn panics_for_no_worker() {
        assert_eq!(
            panic_of(|| _ = history::workers(0)).as_deref(),
            Some("history: workers(0) searches nothing")
        );
    }
}
