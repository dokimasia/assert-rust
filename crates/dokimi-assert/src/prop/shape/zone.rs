//! The zone list and every offset change of its zones, as the definition's
//! zone table states them.
//!
//! The zone shape samples the list, and the zoned-date-time and wall-time
//! shapes take a value near one of a zone's changes in one case of four. The
//! list is in the definition's order, UTC first, so a zone shrinks to UTC.

use crate::json::{self, Json};
use std::sync::LazyLock;

/// The definition's zone table, which `spec sync` copies beside this file.
const TABLE: &str = include_str!("zones.json");

/// The levels of arrays and objects that the table nests.
const TABLE_DEPTH: usize = 5;

/// One offset change: its first second, and the offsets before and after
/// it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Change {
    /// The first second of the change, since 1970-01-01T00:00:00Z.
    pub(crate) at: i128,
    /// The offset before the change, in seconds east of UTC.
    pub(crate) before: i128,
    /// The offset from the change on, in seconds east of UTC.
    pub(crate) after: i128,
}

/// A zone of the list, with its offset changes in time order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Zone {
    /// The name of the zone in the IANA time zone database.
    pub(crate) name: String,
    /// The offset changes of the zone, in time order.
    pub(crate) changes: Vec<Change>,
}

/// The zone list, which the first shape that reads it parses.
static ZONES: LazyLock<Vec<Zone>> = LazyLock::new(|| read(TABLE));

/// Returns the zone list, in the definition's order.
pub(crate) fn zones() -> &'static [Zone] {
    &ZONES
}

/// Returns the zones of the table. The table is the definition's, and `spec
/// check` compares its digest with the vendored manifest, so it parses.
fn read(text: &str) -> Vec<Zone> {
    let table = json::parse(text, TABLE_DEPTH).unwrap_or(Json::Null);
    let listed = table
        .get("zones")
        .and_then(Json::as_array)
        .unwrap_or_default();
    listed
        .iter()
        .map(|zone| Zone {
            name: zone
                .get("name")
                .and_then(Json::as_str)
                .unwrap_or_default()
                .to_owned(),
            changes: zone
                .get("changes")
                .and_then(Json::as_array)
                .unwrap_or_default()
                .iter()
                .map(|change| {
                    let part = |at: usize| {
                        change
                            .as_array()
                            .and_then(|parts| parts.get(at))
                            .and_then(Json::as_int)
                            .unwrap_or_default()
                    };
                    Change {
                        at: part(0),
                        before: part(1),
                        after: part(2),
                    }
                })
                .collect(),
        })
        .collect()
}
