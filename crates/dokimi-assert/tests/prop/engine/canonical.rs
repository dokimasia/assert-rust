//! The spec of `prop::engine::canonical`, the equality of decoded values
//! and the typed literal of a value of a type that the library knows.

use crate::helpers::records;
use dokimi_assert::prop::engine::harness;
use dokimi_assert::prop::{self, Generator};
use dokimi_assert::{Recorder, Value, check};
use serde_json::json;
use std::fmt::Debug;
use std::time::Duration;

/// Returns the typed literal of the simplest value of `generator`, as the
/// counterexample of a property that every value fails states it.
fn drawn<T: Debug + 'static>(generator: &Generator<T>) -> serde_json::Value {
    let seat = Recorder::new();
    prop::for_all(
        &seat,
        "no value passes",
        prop::hermetic().seed(1).store("").shrink(0),
        |c| {
            let _ = c.draw(generator, "value");
            check::is_true(c, false, "no value passes");
        },
    );
    records(&seat)[0]["detail"]["counterexample"][0]["value"].clone()
}

mod same {
    use super::*;

    #[test]
    fn compares_an_opaque_value_by_its_text() {
        let opaque = Value::Opaque("Point { x: 1 }".to_owned());
        let generator = prop::sampled_from([opaque.clone()]);
        assert!(harness::invert(&generator, &opaque).is_ok());
        assert!(harness::invert(&generator, &Value::Opaque("Point { x: 2 }".to_owned())).is_err());
    }
}

mod literal_any {
    use super::*;

    #[test]
    fn states_bytes_and_a_duration_as_their_typed_literals() {
        let bytes = prop::integer(0..=1_u8).map(|n| vec![n, 255]);
        assert_eq!(drawn(&bytes), json!({"type": "bytes", "value": "00ff"}));
        let duration = prop::integer(0..=1_u64).map(|n| Duration::from_millis(n + 1));
        assert_eq!(drawn(&duration), json!({"type": "int", "value": 1_000_000}));
    }
}
