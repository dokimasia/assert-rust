//! The corpus form of choices, values and seeds, and the comparison of JSON.
//!
//! A vector writes a choice as a JSON integer, or as a decimal string beyond
//! 2^53 - 1 in magnitude; `{"float": …}` with a number or a float's name; or
//! `{"sequence": […]}`. It writes a value as a typed literal and a seed as a
//! decimal string.

use dokimi_assert::Value;
use dokimi_assert::prop::engine::choice::Choice;
use serde_json::{Value as Json, json};

/// The largest integer that a JSON reader in JavaScript keeps exact.
const SAFE: i128 = (1 << 53) - 1;

/// Returns an integer as a vector writes it: a number within ±(2^53 - 1),
/// and its decimal text beyond.
#[must_use]
pub fn plain(held: i128) -> Json {
    if (-SAFE..=SAFE).contains(&held) {
        i64::try_from(held).map_or(Json::Null, Json::from)
    } else {
        Json::from(held.to_string())
    }
}

/// Returns a float as a vector writes it, with NaN and the infinities by
/// name.
#[must_use]
pub fn plain_float(held: f64) -> Json {
    if held.is_nan() {
        json!("NaN")
    } else if held.is_infinite() {
        json!(if held > 0.0 { "Inf" } else { "-Inf" })
    } else {
        json!(held)
    }
}

/// Returns the corpus form of one choice.
#[must_use]
pub fn choice_literal(choice: &Choice) -> Json {
    match choice {
        Choice::Integer(held) => plain(*held),
        Choice::Float(held) => json!({ "float": plain_float(*held) }),
        Choice::Sequence(elements) => json!({ "sequence": elements }),
    }
}

/// Returns the corpus forms of choices.
#[must_use]
pub fn choices_literal(choices: &[Choice]) -> Json {
    Json::Array(choices.iter().map(choice_literal).collect())
}

/// Returns an integer that a vector states as a number or as decimal text.
#[must_use]
pub fn integer(written: &Json) -> Option<i128> {
    match written {
        Json::Number(number) => number
            .as_i64()
            .map(i128::from)
            .or_else(|| number.as_u64().map(i128::from)),
        Json::String(text) => text.parse().ok(),
        _ => None,
    }
}

/// Returns a float that a vector states as a number or by name.
#[must_use]
pub fn number(written: &Json) -> Option<f64> {
    match written {
        Json::String(text) => match text.as_str() {
            "NaN" => Some(f64::NAN),
            "Inf" => Some(f64::INFINITY),
            "-Inf" => Some(f64::NEG_INFINITY),
            _ => None,
        },
        Json::Number(number) => number.as_f64(),
        _ => None,
    }
}

/// Returns the choice that a corpus form states.
#[must_use]
pub fn parse_choice(written: &Json) -> Option<Choice> {
    if let Json::Object(members) = written {
        if members.len() == 1 {
            if let Some(float) = members.get("float") {
                return number(float).map(Choice::Float);
            }
            if let Some(Json::Array(elements)) = members.get("sequence") {
                return elements
                    .iter()
                    .map(|element| integer(element).and_then(|held| u64::try_from(held).ok()))
                    .collect::<Option<Vec<u64>>>()
                    .map(Choice::Sequence);
            }
        }
        return None;
    }
    integer(written).map(Choice::Integer)
}

/// Returns the choices that a list of corpus forms states.
#[must_use]
pub fn parse_choices(written: &Json) -> Option<Vec<Choice>> {
    written.as_array()?.iter().map(parse_choice).collect()
}

/// Returns a vector's seed, which it states as decimal text.
#[must_use]
pub fn seed(written: &Json) -> Option<u64> {
    written.as_str()?.parse().ok()
}

/// Returns the typed literal of a value, as the definition encodes it.
#[must_use]
pub fn literal(value: &Value) -> Json {
    match value {
        Value::Null => json!({"type": "null"}),
        Value::Bool(held) => json!({"type": "bool", "value": held}),
        Value::Int(held) => json!({"type": "int", "value": plain(*held)}),
        Value::Uint(held) => json!({"type": "int", "value": held.to_string()}),
        Value::Float(held) => json!({"type": "float", "value": plain_float(*held)}),
        Value::String(text) => json!({"type": "string", "value": text}),
        Value::Bytes(bytes) => json!({"type": "bytes", "value": hex(bytes)}),
        Value::List(items) => list(items),
        Value::Map(entries) => json!({
            "type": "map",
            "entries": entries.iter().map(|(key, value)| json!([literal(key), literal(value)])).collect::<Vec<_>>(),
        }),
        Value::Record(fields) => json!({
            "type": "record",
            "fields": fields.iter().map(|(name, value)| json!([name, literal(value)])).collect::<Vec<_>>(),
        }),
        Value::Variant { name, payload } => match payload {
            Some(payload) => json!({"type": "variant", "name": name, "payload": literal(payload)}),
            None => json!({"type": "variant", "name": name}),
        },
        other => json!({"type": "opaque", "text": format!("{other:?}")}),
    }
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut text, byte| {
        // Writing to a String cannot fail.
        let _ = write!(text, "{byte:02x}");
        text
    })
}

/// Returns a list's literal: `of` and `value` for a non-empty list of one
/// scalar type, and `items` for any other list.
fn list(items: &[Value]) -> Json {
    let kind = |value: &Value| match value {
        Value::Bool(_) => Some("bool"),
        Value::Int(_) | Value::Uint(_) => Some("int"),
        Value::Float(_) => Some("float"),
        Value::String(_) => Some("string"),
        _ => None,
    };
    let first = items.first().and_then(kind);
    if let Some(of) = first.filter(|of| items.iter().all(|item| kind(item) == Some(of))) {
        let plain_items: Vec<Json> = items
            .iter()
            .map(|item| literal(item)["value"].clone())
            .collect();
        return json!({"type": "list", "of": of, "value": plain_items});
    }
    json!({"type": "list", "items": items.iter().map(literal).collect::<Vec<_>>()})
}

/// Returns `true` when two JSON values are the same: objects with the same
/// members in any order, arrays with the same items in order, floats with the
/// same bits, and numbers that are equal integers.
#[must_use]
pub fn same_json(a: &Json, b: &Json) -> bool {
    match (a, b) {
        (Json::Object(a), Json::Object(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(key, value)| b.get(key).is_some_and(|other| same_json(value, other)))
        }
        (Json::Array(a), Json::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(x, y)| same_json(x, y))
        }
        (Json::Number(x), Json::Number(y)) => {
            if x.is_f64() || y.is_f64() {
                x.as_f64().map(f64::to_bits) == y.as_f64().map(f64::to_bits)
            } else {
                x == y
            }
        }
        _ => a == b,
    }
}
