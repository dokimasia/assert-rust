//! The helpers that more than one spec of the binary uses.

use dokimi_assert::Recorder;
use std::path::Path;

/// Returns the call records of `seat` as JSON values, in the order of their
/// numbers.
pub(crate) fn records(seat: &Recorder) -> Vec<serde_json::Value> {
    seat.records()
        .iter()
        .map(|line| serde_json::from_str(line).expect("a call record is JSON"))
        .collect()
}

/// Returns `true` when the first failure of `seat` names a call site in the
/// specs of this crate.
pub(crate) fn names_a_spec(seat: &Recorder) -> bool {
    let failures = seat.failures();
    let at = failures[0]
        .location()
        .expect("a failure states its call site");
    Path::new(at.file())
        .components()
        .any(|part| part.as_os_str() == "tests")
}
