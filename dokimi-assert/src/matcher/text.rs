//! Assertions about text.

use super::report::{Mode, fail};
use crate::seat::Seat;
use regex::Regex;

/// Report when got does not start with prefix.
#[track_caller]
pub fn has_prefix(seat: &dyn Seat, mode: Mode, got: &str, prefix: &str, msg: &str) {
    seat.helper();
    if !got.starts_with(prefix) {
        fail(
            seat,
            mode,
            "has-prefix",
            msg,
            vec![
                ("got", format!("{got:?}").into()),
                ("prefix", format!("{prefix:?}").into()),
            ],
        );
    }
}

/// Report when got does not end with suffix.
#[track_caller]
pub fn has_suffix(seat: &dyn Seat, mode: Mode, got: &str, suffix: &str, msg: &str) {
    seat.helper();
    if !got.ends_with(suffix) {
        fail(
            seat,
            mode,
            "has-suffix",
            msg,
            vec![
                ("got", format!("{got:?}").into()),
                ("suffix", format!("{suffix:?}").into()),
            ],
        );
    }
}

/// Report when got does not match the pattern.
///
/// The pattern is searched rather than anchored: use `^` and `$` where you mean the whole
/// value. A pattern that does not compile is reported as the failure, so a typo in a
/// pattern does not read like a failing subject.
#[track_caller]
pub fn matches(seat: &dyn Seat, mode: Mode, got: &str, pattern: &str, msg: &str) {
    seat.helper();
    match Regex::new(pattern) {
        Err(_) => {
            fail(
                seat,
                mode,
                "matches",
                msg,
                vec![
                    ("got", format!("{got:?}").into()),
                    ("pattern", format!("{pattern:?}").into()),
                ],
            );
        }
        Ok(compiled) => {
            if !compiled.is_match(got) {
                fail(
                    seat,
                    mode,
                    "matches",
                    msg,
                    vec![
                        ("got", format!("{got:?}").into()),
                        ("pattern", format!("{pattern:?}").into()),
                    ],
                );
            }
        }
    }
}
