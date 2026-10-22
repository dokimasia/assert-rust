//! The spec of `prop::shape::zone`, the zone list of the definition's zone
//! table.

use dokimi_assert::Value;
use dokimi_assert::prop;
use dokimi_assert::prop::engine::choice::Choice;
use dokimi_assert::prop::engine::harness;

mod zones {
    use super::*;

    #[test]
    fn lists_the_zones_of_the_table_in_its_order_with_utc_first() {
        let zone = prop::of_shape(r#"{"shape": "zone"}"#).expect("the shape file reads");
        let named = |at: i128| harness::decode(&zone, vec![Choice::Integer(at)]).1;
        assert_eq!(named(0), Some(Value::from("UTC")));
        assert_eq!(named(1), Some(Value::from("Europe/Amsterdam")));
        assert!(named(15).is_some());
    }

    #[test]
    fn states_the_offset_changes_of_a_zone() {
        let zoned = prop::of_shape(r#"{"shape": "zoned-date-time", "unit": "s"}"#)
            .expect("the shape file reads");
        let choices = vec![
            Choice::Integer(1),
            Choice::Integer(1),
            Choice::Integer(0),
            Choice::Integer(0),
        ];
        let (_, value) = harness::decode(&zoned, choices);
        let Some(Value::Record(fields)) = value else {
            panic!("a zoned date and time is a record");
        };
        assert_eq!(
            fields[1],
            ("zone".to_owned(), Value::from("Europe/Amsterdam"))
        );
        let instant = Value::Record(vec![
            ("seconds".to_owned(), Value::Int(-1_740_355_200)),
            ("units".to_owned(), Value::Int(0)),
        ]);
        assert_eq!(fields[0], ("instant".to_owned(), instant));
    }
}
