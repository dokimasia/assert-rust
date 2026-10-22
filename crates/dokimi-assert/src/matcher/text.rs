// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! Prefixes, suffixes and patterns of text.

use crate::pattern;
use crate::seat::Seat;
use crate::value::Value;
use crate::verdict::{Mode, Run};

/// Reports whether `got` starts with `prefix`.
#[track_caller]
pub(crate) fn has_prefix(seat: &dyn Seat, mode: Mode, got: &str, prefix: &str, msg: &str) {
    seat.helper();
    let run = Run::of(seat);
    if !got.starts_with(prefix) {
        run.fail(
            mode,
            "has-prefix",
            msg,
            vec![("got", Value::from(got)), ("prefix", Value::from(prefix))],
        );
        return;
    }
    run.pass(mode, "has-prefix", msg);
}

/// Reports whether `got` ends with `suffix`.
#[track_caller]
pub(crate) fn has_suffix(seat: &dyn Seat, mode: Mode, got: &str, suffix: &str, msg: &str) {
    seat.helper();
    let run = Run::of(seat);
    if !got.ends_with(suffix) {
        run.fail(
            mode,
            "has-suffix",
            msg,
            vec![("got", Value::from(got)), ("suffix", Value::from(suffix))],
        );
        return;
    }
    run.pass(mode, "has-suffix", msg);
}

/// Reports whether `pattern`, a pattern of the portable subset, matches
/// anywhere in `got`. A pattern outside the subset fails, with the text of
/// the parser's refusal as the reason.
#[track_caller]
pub(crate) fn matches(seat: &dyn Seat, mode: Mode, got: &str, pattern: &str, msg: &str) {
    seat.helper();
    let run = Run::of(seat);
    let reason = match pattern::compile(pattern) {
        Ok(compiled) if compiled.is_match(got) => {
            run.pass(mode, "matches", msg);
            return;
        }
        Ok(_) => Value::Null,
        Err(refused) => Value::from(refused.to_string()),
    };
    run.fail(
        mode,
        "matches",
        msg,
        vec![
            ("got", Value::from(got)),
            ("pattern", Value::from(pattern)),
            ("reason", reason),
        ],
    );
}
