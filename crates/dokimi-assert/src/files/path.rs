// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The assertions of the entry at one path.
//!
//! Each assertion reads the entry at a path of the operating system itself,
//! and follows no link there. The platform resolves the names before the
//! last one, and follows a link among them. Nothing is at a path below a
//! file.

use super::entry::{Entry, Form, PERMISSIONS};
use super::reader::read_path;
use super::tree::Fault;
use crate::seat::Seat;
use crate::value::Value;
use crate::verdict::{Mode, Run};
use std::path::Path;

/// The detail of a failure of an assertion of one path.
type Detail = Vec<(&'static str, Value)>;

/// Ends the test when an entry is at `path`: a file, a directory or a link,
/// a link whose target is missing included.
///
/// The record states `got`, the kind of the entry there: `file`,
/// `directory` or `link`. An entry that is no file, directory or link, and
/// an error of the file system other than one that finds nothing at `path`,
/// end the call with a fault.
#[track_caller]
pub fn absent(seat: &dyn Seat, path: impl AsRef<Path>, msg: &str) {
    seat.helper();
    let read = read_path(path.as_ref(), false);
    report(
        &Run::of(seat),
        "path-absent",
        "files::absent",
        msg,
        read,
        |entry| entry.map(|entry| vec![("got", Value::from(entry.kind()))]),
    );
}

/// Ends the test when no file is at `path`. A link to a file is a link.
///
/// The record states `got`, the kind of the entry there, or null for none.
/// The reading and the faults are those of [`absent`].
#[track_caller]
pub fn is_file(seat: &dyn Seat, path: impl AsRef<Path>, msg: &str) {
    seat.helper();
    let read = read_path(path.as_ref(), false);
    report(
        &Run::of(seat),
        "is-file",
        "files::is_file",
        msg,
        read,
        kind_is("file"),
    );
}

/// Ends the test when no directory is at `path`. A link to a directory is a
/// link.
///
/// The record states `got`, the kind of the entry there, or null for none.
/// The reading and the faults are those of [`absent`].
#[track_caller]
pub fn is_dir(seat: &dyn Seat, path: impl AsRef<Path>, msg: &str) {
    seat.helper();
    let read = read_path(path.as_ref(), false);
    report(
        &Run::of(seat),
        "is-dir",
        "files::is_dir",
        msg,
        read,
        kind_is("directory"),
    );
}

/// Ends the test when no symbolic link whose target is `target` is at
/// `path`.
///
/// The target compares as text, with `/` between its names on every
/// platform, and nothing follows the link. The record states `want`, the
/// target, `got`, the target of the link there or null when no link is
/// there, and `kind`, the kind of the entry there or null for none. The
/// reading and the faults are those of [`absent`].
#[track_caller]
pub fn links_to(seat: &dyn Seat, path: impl AsRef<Path>, target: &str, msg: &str) {
    seat.helper();
    let read = read_path(path.as_ref(), false);
    report(
        &Run::of(seat),
        "links-to",
        "files::links_to",
        msg,
        read,
        |entry| {
            let got = entry.and_then(|entry| match &entry.form {
                Form::Link(target) => Some(target.as_str()),
                _ => None,
            });
            (got != Some(target)).then(|| {
                vec![
                    ("want", Value::from(target)),
                    ("got", Value::from(got)),
                    ("kind", Value::from(entry.map(Entry::kind))),
                ]
            })
        },
    );
}

/// Ends the test when no file whose bytes equal `want` is at `path`.
///
/// It compares bytes, so a file that ends in `\r\n` does not equal text that
/// ends in `\n`. The record states `want`, `got`, the content of the file
/// there or null when no file is there, and `kind`, the kind of the entry
/// there or null for none. A content is text when it is UTF-8, and bytes
/// otherwise. The reading and the faults are those of [`absent`].
#[track_caller]
pub fn has_content(seat: &dyn Seat, path: impl AsRef<Path>, want: impl AsRef<[u8]>, msg: &str) {
    seat.helper();
    let want = want.as_ref();
    let read = read_path(path.as_ref(), true);
    report(
        &Run::of(seat),
        "has-content",
        "files::has_content",
        msg,
        read,
        |entry| {
            let got = entry.and_then(|entry| match &entry.form {
                Form::File(content) => Some(content.as_slice()),
                _ => None,
            });
            (got != Some(want)).then(|| {
                vec![
                    ("want", text_or_bytes(want)),
                    ("got", got.map_or(Value::Null, text_or_bytes)),
                    ("kind", Value::from(entry.map(Entry::kind))),
                ]
            })
        },
    );
}

/// Ends the test when no file or directory whose nine permission bits equal
/// `mode` is at `path`. A link has no mode.
///
/// ```no_run
/// use dokimi_assert::{Collector, files};
///
/// let t = Collector::new();
/// files::has_mode(&t, "/home/me/.config/tool/key", 0o600, "the key is private");
/// ```
///
/// The record states `want`, the mode, `got`, the permission bits of the
/// file or the directory there or null when neither is there, and `kind`,
/// the kind of the entry there or null for none. A `mode` with a bit beyond
/// the nine permission bits ends the call with a fault, and so does a
/// platform that records no permission bits, such as Windows. The reading
/// and the other faults are those of [`absent`].
#[track_caller]
pub fn has_mode(seat: &dyn Seat, path: impl AsRef<Path>, mode: u32, msg: &str) {
    seat.helper();
    let refused = (mode & !PERMISSIONS != 0)
        .then(|| {
            Fault::new(format!(
                "the mode {mode:#o} has a bit beyond the nine permission bits"
            ))
        })
        .or_else(unrecorded);
    let read = refused.map_or_else(|| read_path(path.as_ref(), false), Err);
    report(
        &Run::of(seat),
        "has-mode",
        "files::has_mode",
        msg,
        read,
        |entry| {
            let got = entry
                .filter(|entry| !matches!(entry.form, Form::Link(_)))
                .map(|entry| entry.mode);
            (got != Some(mode)).then(|| {
                vec![
                    ("want", Value::from(mode)),
                    ("got", Value::from(got)),
                    ("kind", Value::from(entry.map(Entry::kind))),
                ]
            })
        },
    );
}

/// Returns the fault of an assertion that reads a mode, on a platform that
/// records no permission bits, and `None` on this platform, which records
/// them.
#[cfg(unix)]
fn unrecorded() -> Option<Fault> {
    None
}

/// Returns the fault of an assertion that reads a mode on this platform,
/// whose file systems record no permission bits.
#[cfg(not(unix))]
fn unrecorded() -> Option<Fault> {
    Some(Fault::new("the file system records no permission bits"))
}

/// Returns the detail of an assertion that an entry of `kind` is at a path:
/// `None` for such an entry, and the kind of any other entry as `got`.
fn kind_is(kind: &'static str) -> impl FnOnce(Option<&Entry>) -> Option<Detail> {
    move |entry| {
        let got = entry.map(Entry::kind);
        (got != Some(kind)).then(|| vec![("got", Value::from(got))])
    }
}

/// Returns `content` as a record states it: text when it is UTF-8, and
/// bytes otherwise.
fn text_or_bytes(content: &[u8]) -> Value {
    std::str::from_utf8(content).map_or_else(|_| Value::Bytes(content.to_vec()), Value::from)
}

/// Reports the verdict of the assertion `id` of one path, whose faults name
/// the function `op`: a fault for a read that failed, a failure with the
/// detail that `detail` returns for the entry read, and a pass when it
/// returns `None`.
#[track_caller]
fn report(
    run: &Run<'_>,
    id: &'static str,
    op: &str,
    msg: &str,
    read: Result<Option<Entry>, Fault>,
    detail: impl FnOnce(Option<&Entry>) -> Option<Detail>,
) {
    match read.map(|entry| detail(entry.as_ref())) {
        Err(fault) => run.fault(Mode::Fatal, id, msg, format_args!("{op}: {fault}")),
        Ok(Some(detail)) => run.fail(Mode::Fatal, id, msg, detail),
        Ok(None) => run.pass(Mode::Fatal, id, msg),
    }
}
