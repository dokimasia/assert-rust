//! The fixture types of the definition, and the shape file that
//! `prop::shape_of` writes for each.
//!
//! Each type has the name of its fixture and the fields that the fixtures
//! vector states. A struct and an enum derive their shapes. `Status` states
//! its values through `prop::register_values`, which [`shape_file`] calls
//! once in the test process.

use dokimi_assert::Value;
use dokimi_assert::prop::{self, Arbitrary, Definitions, Shape, ShapeError};
use serde_json::{Map, Value as Json};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::net::{IpAddr, Ipv4Addr};
use std::sync::Once;
use std::time::SystemTime;

#[derive(Arbitrary)]
struct Flag {
    enabled: bool,
}

#[derive(Arbitrary)]
struct Counter {
    count: i32,
}

#[derive(Arbitrary)]
struct Ratio {
    value: f64,
}

#[derive(Arbitrary)]
struct Initial {
    letter: char,
}

#[derive(Arbitrary)]
struct Person {
    name: String,
}

#[derive(Arbitrary)]
struct Blob {
    payload: Vec<u8>,
}

#[derive(Arbitrary)]
struct Readings {
    values: Vec<i32>,
}

#[derive(Arbitrary)]
struct Colour {
    rgb: [u8; 3],
}

#[derive(Arbitrary)]
struct Tags {
    tags: BTreeSet<String>,
}

#[derive(Arbitrary)]
struct Inventory {
    counts: BTreeMap<String, i32>,
}

#[derive(Arbitrary)]
struct Memo {
    note: Option<String>,
}

#[derive(Arbitrary)]
struct Line {
    sku: String,
    qty: i32,
}

#[derive(Arbitrary)]
struct Order {
    id: u32,
    lines: Vec<Line>,
    note: Option<String>,
}

#[derive(Arbitrary)]
enum Payment {
    #[prop(rename = "pending")]
    Pending,
    #[prop(rename = "paid")]
    Paid(i64),
    #[prop(rename = "refunded")]
    Refunded(String),
}

/// The state of an order, which reads as the text of its name.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Status {
    Pending,
    Paid,
    Shipped,
}

impl Status {
    /// The values of the type, in the order of the fixture.
    const ALL: [Self; 3] = [Self::Pending, Self::Paid, Self::Shipped];

    fn name(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Paid => "paid",
            Self::Shipped => "shipped",
        }
    }
}

impl Arbitrary for Status {
    fn shape(_: &mut Definitions) -> Shape {
        Shape::string()
    }

    fn from_value(value: Value) -> Result<Self, ShapeError> {
        let text = String::from_value(value)?;
        Self::ALL
            .into_iter()
            .find(|status| status.name() == text)
            .ok_or_else(|| ShapeError::new(format!("{text:?} is no status")))
    }

    fn to_value(&self) -> Value {
        Value::from(self.name())
    }
}

#[derive(Arbitrary)]
struct Tree {
    value: i32,
    children: Vec<Tree>,
}

#[derive(Arbitrary)]
struct Account {
    id: uuid::Uuid,
}

#[derive(Arbitrary)]
struct Host {
    address: IpAddr,
}

#[derive(Arbitrary)]
struct Price {
    #[prop(scale = 2)]
    amount: rust_decimal::Decimal,
}

#[derive(Arbitrary)]
struct Event {
    #[prop(unit = "us")]
    at: SystemTime,
}

#[derive(Arbitrary)]
struct Due {
    day: jiff::civil::Date,
}

#[derive(Arbitrary)]
struct Opening {
    #[prop(unit = "us")]
    opens: jiff::civil::Time,
}

#[derive(Arbitrary)]
struct Appointment {
    #[prop(unit = "us")]
    starts: jiff::civil::DateTime,
}

#[derive(Arbitrary)]
struct Timeout {
    #[prop(unit = "us")]
    after: jiff::SignedDuration,
}

#[derive(Arbitrary)]
struct Offset {
    offset: jiff::tz::Offset,
}

#[derive(Arbitrary)]
struct Locale {
    zone: jiff::tz::TimeZone,
}

#[derive(Arbitrary)]
struct Meeting {
    #[prop(unit = "us")]
    starts: jiff::Zoned,
}

#[derive(Arbitrary)]
struct Quantity {
    #[prop(min = 1, max = 99)]
    qty: i32,
}

#[derive(Arbitrary)]
struct Batch {
    #[prop(min_size = 1, max_size = 10)]
    lines: Vec<String>,
}

#[derive(Arbitrary)]
struct Sku {
    #[prop(pattern = "[A-Z]{3}-[0-9]{4}")]
    code: String,
}

#[derive(Arbitrary)]
struct Hex {
    #[prop(alphabet = "0123456789abcdef")]
    digits: String,
}

#[derive(Arbitrary)]
struct Measurement {
    #[prop(allow_nan, allow_infinity)]
    value: f64,
}

#[derive(Arbitrary)]
struct Stamp {
    #[prop(unit = "ms")]
    at: SystemTime,
}

#[derive(Arbitrary)]
struct Fee {
    #[prop(scale = 4, min = "0.0001")]
    amount: rust_decimal::Decimal,
}

#[derive(Arbitrary)]
struct Gateway {
    address: Ipv4Addr,
}

/// Registers the values of `Status` once in the test process.
static REGISTER: Once = Once::new();

/// Returns the shape file that `prop::shape_of` writes for the fixture type
/// named `fixture`, or `None` for a name that names no fixture type.
pub fn shape_file(fixture: &str) -> Option<Result<String, ShapeError>> {
    REGISTER.call_once(|| prop::register_values(Status::ALL));
    let shape_of: fn() -> Result<String, ShapeError> = match fixture {
        "flag" => prop::shape_of::<Flag>,
        "counter" => prop::shape_of::<Counter>,
        "ratio" => prop::shape_of::<Ratio>,
        "initial" => prop::shape_of::<Initial>,
        "person" => prop::shape_of::<Person>,
        "blob" => prop::shape_of::<Blob>,
        "readings" => prop::shape_of::<Readings>,
        "colour" => prop::shape_of::<Colour>,
        "tags" => prop::shape_of::<Tags>,
        "inventory" => prop::shape_of::<Inventory>,
        "memo" => prop::shape_of::<Memo>,
        "order" => prop::shape_of::<Order>,
        "payment" => prop::shape_of::<Payment>,
        "status" => prop::shape_of::<Status>,
        "tree" => prop::shape_of::<Tree>,
        "account" => prop::shape_of::<Account>,
        "host" => prop::shape_of::<Host>,
        "price" => prop::shape_of::<Price>,
        "event" => prop::shape_of::<Event>,
        "due" => prop::shape_of::<Due>,
        "opening" => prop::shape_of::<Opening>,
        "appointment" => prop::shape_of::<Appointment>,
        "timeout" => prop::shape_of::<Timeout>,
        "offset" => prop::shape_of::<Offset>,
        "locale" => prop::shape_of::<Locale>,
        "meeting" => prop::shape_of::<Meeting>,
        "quantity" => prop::shape_of::<Quantity>,
        "batch" => prop::shape_of::<Batch>,
        "sku" => prop::shape_of::<Sku>,
        "hex" => prop::shape_of::<Hex>,
        "measurement" => prop::shape_of::<Measurement>,
        "stamp" => prop::shape_of::<Stamp>,
        "fee" => prop::shape_of::<Fee>,
        "gateway" => prop::shape_of::<Gateway>,
        _ => return None,
    };
    Some(shape_of())
}

/// Returns a shape file without its source, with each definition that a ref
/// names renamed `#0`, `#1` and on, in the order in which a walk of the shape
/// first refers to it.
///
/// The walk visits the members of an object in the order of their keys, the
/// items of a list in order, and a definition when a ref first names it. A
/// definition that no ref names keeps its name. Two languages name a
/// definition after their own types, so the names do not compare.
#[must_use]
pub fn normal(mut document: Json) -> Json {
    let Some(object) = document.as_object_mut() else {
        return document;
    };
    object.remove("source");
    let definitions = object
        .remove("definitions")
        .and_then(|definitions| definitions.as_object().cloned())
        .unwrap_or_default();
    let mut names = HashMap::new();
    let mut renamed = Map::new();
    walk(&mut document, &definitions, &mut names, &mut renamed);
    for (name, definition) in definitions {
        if !names.contains_key(&name) {
            renamed.insert(name, definition);
        }
    }
    if !renamed.is_empty() {
        if let Some(object) = document.as_object_mut() {
            object.insert("definitions".to_owned(), Json::Object(renamed));
        }
    }
    document
}

/// Renames the refs of `node`, and walks each definition that a ref names
/// for the first time into `renamed`.
fn walk(
    node: &mut Json,
    definitions: &Map<String, Json>,
    names: &mut HashMap<String, String>,
    renamed: &mut Map<String, Json>,
) {
    match node {
        Json::Object(object) => {
            if object.get("shape").and_then(Json::as_str) == Some("ref") {
                if let Some(name) = object.get("name").and_then(Json::as_str).map(str::to_owned) {
                    let new = if let Some(new) = names.get(&name) {
                        new.clone()
                    } else {
                        let new = format!("#{}", names.len());
                        names.insert(name.clone(), new.clone());
                        let mut definition = definitions.get(&name).cloned().unwrap_or(Json::Null);
                        walk(&mut definition, definitions, names, renamed);
                        renamed.insert(new.clone(), definition);
                        new
                    };
                    object.insert("name".to_owned(), Json::String(new));
                }
            }
            let mut keys: Vec<String> = object.keys().cloned().collect();
            keys.sort();
            for key in keys {
                if let Some(member) = object.get_mut(&key) {
                    walk(member, definitions, names, renamed);
                }
            }
        }
        Json::Array(items) => {
            for item in items {
                walk(item, definitions, names, renamed);
            }
        }
        _ => {}
    }
}
