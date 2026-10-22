//! The spec of `history::interval`, which builds a history from calls with a
//! start and an end on one clock.

use dokimi_assert::Value;
use dokimi_assert::history::{self, Ending, Interval, Kind};

/// Returns the call of `client` from `start` to its end, without arguments
/// and keys.
fn interval(client: usize, operation: &str, start: i128, end: Option<(i128, Ending)>) -> Interval {
    Interval {
        client,
        operation: operation.to_owned(),
        args: Vec::new(),
        keys: Vec::new(),
        start,
        end,
    }
}

mod from_intervals {
    use super::*;

    #[test]
    fn records_an_invocation_before_a_completion_at_one_time() {
        let entries = [
            interval(0, "write", 0, Some((5, Ending::Ok(Value::Null)))),
            interval(1, "read", 5, Some((8, Ending::Ok(Value::from(1))))),
        ];
        let h = history::from_intervals(&entries).expect("the entries form a history");
        let order: Vec<(usize, bool)> = h
            .events()
            .iter()
            .map(|event| (event.client, matches!(event.kind, Kind::Invoke { .. })))
            .collect();
        assert_eq!(order, [(0, true), (1, true), (0, false), (1, false)]);
    }

    #[test]
    fn records_each_ending_and_a_pending_call() {
        let entries = [
            interval(0, "a", 0, Some((1, Ending::Ok(Value::from(1))))),
            interval(1, "b", 2, Some((3, Ending::Fail("refused".to_owned())))),
            interval(2, "c", 4, Some((5, Ending::Unknown("timeout".to_owned())))),
            interval(3, "d", 6, None),
        ];
        let h = history::from_intervals(&entries).expect("the entries form a history");
        let kinds: Vec<Kind> = h.events().into_iter().map(|event| event.kind).collect();
        assert_eq!(kinds.len(), 7);
        assert_eq!(
            kinds[1],
            Kind::Ok {
                output: Value::from(1)
            }
        );
        assert_eq!(
            kinds[3],
            Kind::Fail {
                error: "refused".to_owned()
            }
        );
        assert_eq!(
            kinds[5],
            Kind::Unknown {
                error: "timeout".to_owned()
            }
        );
        assert!(matches!(&kinds[6], Kind::Invoke { operation, .. } if operation == "d"));
    }

    #[test]
    fn takes_overlapping_calls_of_two_clients() {
        let entries = [
            interval(0, "a", 0, Some((9, Ending::Ok(Value::Null)))),
            interval(1, "b", 1, Some((2, Ending::Ok(Value::Null)))),
        ];
        assert_eq!(
            history::from_intervals(&entries).map(|h| h.events().len()),
            Ok(4)
        );
    }

    #[test]
    fn refuses_an_entry_that_ends_before_it_starts() {
        let entries = [interval(0, "a", 2, Some((1, Ending::Ok(Value::Null))))];
        let refused =
            history::from_intervals(&entries).expect_err("the entry ends before it starts");
        assert_eq!(refused.entry(), 0);
        assert_eq!(
            refused.to_string(),
            "history: entry 0 ends at 1, before it starts at 2"
        );
    }

    #[test]
    fn refuses_a_key_without_a_typed_literal() {
        let mut entry = interval(0, "a", 0, None);
        entry.keys = vec![Value::Opaque("handle".to_owned())];
        let refused = history::from_intervals(&[entry]).expect_err("the key has no typed literal");
        assert_eq!(refused.entry(), 0);
        assert!(
            refused
                .to_string()
                .ends_with("which no typed literal states")
        );
    }

    #[test]
    fn refuses_an_entry_that_shares_an_instant_with_an_earlier_entry_of_its_client() {
        let entries = [
            interval(0, "a", 0, Some((5, Ending::Ok(Value::Null)))),
            interval(0, "b", 5, Some((9, Ending::Ok(Value::Null)))),
        ];
        let refused = history::from_intervals(&entries).expect_err("the closed intervals share 5");
        assert_eq!(refused.entry(), 1);
        assert_eq!(
            refused.to_string(),
            "history: entry 1 overlaps entry 0 of client 0"
        );
    }

    #[test]
    fn refuses_an_entry_after_a_pending_entry_of_its_client() {
        let entries = [interval(0, "a", 0, None), interval(0, "b", 100, None)];
        let refused = history::from_intervals(&entries).expect_err("a pending entry never ends");
        assert_eq!(refused.entry(), 1);
    }
}
