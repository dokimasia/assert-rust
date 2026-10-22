// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The comparisons of the tree in a directory with a wanted tree.

use super::entry::{Entry, PERMISSIONS, execute_bit};
use super::reader::{RECORDED_BITS, read_tree};
use super::tree::Tree;
use crate::seat::Seat;
use crate::value::Value;
use crate::verdict::{Mode, Run};
use std::path::Path;

/// The most paths that a record lists.
const MAX_PATHS: usize = 64;

/// Returns the paths at which `got`, a tree read from a directory, differs
/// from `want`, in path order. `want` implies the parents of its entries. A
/// path differs when one of the two trees has it and the other does not, or
/// when the entries at it differ. With `contains` set, a path that `want`
/// lacks is no difference.
pub(crate) fn differing(want: &Tree, got: &Tree, contains: bool) -> Vec<String> {
    let wanted = want.full();
    let mut out: Vec<String> = wanted
        .entries
        .iter()
        .filter(|(path, entry)| differs(entry, got.get(path)))
        .map(|(path, _)| path.clone())
        .collect();
    if !contains {
        out.extend(
            got.entries
                .keys()
                .filter(|path| !wanted.entries.contains_key(*path))
                .cloned(),
        );
    }
    out.sort();
    out
}

/// Returns `true` when `got`, the entry read at a path, differs from `want`,
/// the wanted entry there.
///
/// Two entries differ when their kinds differ, when two files have other
/// content or two links other targets, when `want` states a mode and `got`
/// has other permission bits, and when `want` is a file that states no mode
/// and the owner's execute bits differ. A platform that records no
/// permission bits compares neither.
pub(crate) fn differs(want: &Entry, got: Option<&Entry>) -> bool {
    got.is_none_or(|got| {
        let compared = if want.stated {
            PERMISSIONS
        } else {
            execute_bit(&want.form)
        };
        want.form != got.form || (want.mode ^ got.mode) & compared & RECORDED_BITS != 0
    })
}

/// The detail of a comparison of trees that fails: the wanted entries and
/// the entries read at the first 64 of the paths that differ, and the number
/// of those paths.
#[derive(Debug)]
pub(crate) struct Record {
    /// The tree of the wanted entries at the paths that differ, and `None`
    /// for a missing golden tree.
    want: Option<Tree>,
    /// The tree of the entries read at those paths.
    got: Tree,
    /// The number of paths that differ.
    differences: usize,
}

impl Record {
    /// Returns the record of a comparison of `got` with `want` at `paths`,
    /// the paths that differ in path order. A missing entry is in `want`
    /// alone, an extra one in `got` alone, and a changed one in both. An
    /// entry of `got` states its mode only where the entry of `want` at its
    /// path states one.
    pub(crate) fn of(want: &Tree, got: &Tree, paths: &[String]) -> Self {
        let wanted = want.full();
        let mut want = Tree::new();
        let mut read = Tree::new();
        for path in paths.iter().take(MAX_PATHS) {
            let stated = wanted.get(path);
            if let Some(entry) = stated {
                want.insert(path.clone(), entry.clone());
            }
            if let Some(entry) = got.get(path) {
                read.insert(path.clone(), entry.as_compared(stated));
            }
        }
        Self {
            want: Some(want),
            got: read,
            differences: paths.len(),
        }
    }

    /// Returns the record of a golden tree that is missing: no wanted tree,
    /// the first 64 entries of `got` in path order, each without its mode,
    /// and the number of the entries of `got`.
    pub(crate) fn missing(got: &Tree) -> Self {
        Self {
            want: None,
            got: got
                .entries
                .iter()
                .take(MAX_PATHS)
                .map(|(path, entry)| (path.clone(), entry.as_compared(None)))
                .collect(),
            differences: got.len(),
        }
    }

    /// Returns the detail of the record as a failure states it.
    pub(crate) fn detail(self) -> Vec<(&'static str, Value)> {
        vec![
            ("want", self.want.map_or(Value::Null, Value::Tree)),
            ("got", Value::Tree(self.got)),
            ("differences", Value::from(self.differences)),
        ]
    }
}

/// Ends the test when the tree in `dir` does not have the entries of `want`,
/// no more and no fewer, each as `want` states it.
///
/// ```
/// use dokimi_assert::{Collector, files};
/// use dokimi_assert::files::Tree;
///
/// let t = Collector::new();
/// let tree = Tree::from([("a/a.go", files::text("package a\n"))]);
/// let dir = files::workspace(&t, &tree);
/// files::equal(&t, &dir, &tree, "the workspace writes the tree");
/// ```
///
/// The comparison reads the directory and follows no link. It compares a
/// mode only where `want` states one, and otherwise the owner's execute bit
/// of a file. The record states `want` and `got`, the trees of the entries
/// at the first 64 paths that differ, and `differences`, the number of those
/// paths. A missing entry is in `want` alone, an extra one in `got` alone,
/// and a changed one in both. An entry of `got` states its mode only where
/// `want` states one.
///
/// A `want` that breaks a rule of a tree, an entry of the directory that is
/// no file, directory or link, and a directory that cannot be read end the
/// call with a fault.
#[track_caller]
pub fn equal(seat: &dyn Seat, dir: impl AsRef<Path>, want: &Tree, msg: &str) {
    seat.helper();
    let run = Run::of(seat);
    compare_read(
        &run,
        "tree-equal",
        "files::equal",
        msg,
        want,
        dir.as_ref(),
        false,
    );
}

/// Ends the test when the tree in `dir` lacks an entry of `want`, or has one
/// in another form than `want` states. The directory may have more entries.
///
/// The comparison, the record and the faults are those of [`equal`], and a
/// path that `want` lacks is no difference.
#[track_caller]
pub fn contains(seat: &dyn Seat, dir: impl AsRef<Path>, want: &Tree, msg: &str) {
    seat.helper();
    let run = Run::of(seat);
    compare_read(
        &run,
        "tree-contains",
        "files::contains",
        msg,
        want,
        dir.as_ref(),
        true,
    );
}

/// Calls `body`, and ends the test when the tree in `dir` after the call
/// differs from the tree in it before the call, modes included.
///
/// The record states the tree before the call as the wanted one. A write of
/// the bytes that a file has, and a change of a timestamp, leave the tree
/// unchanged. A panic of `body` panics past the assertion, which then
/// compares nothing. The record and the faults are those of [`equal`].
#[track_caller]
pub fn unchanged(seat: &dyn Seat, dir: impl AsRef<Path>, body: impl FnOnce(), msg: &str) {
    seat.helper();
    let run = Run::of(seat);
    let dir = dir.as_ref();
    let read = read_tree(dir, true).and_then(|before| {
        body();
        read_tree(dir, true).map(|after| (before, after))
    });
    match read {
        Err(fault) => run.fault(
            Mode::Fatal,
            "tree-unchanged",
            msg,
            format_args!("files::unchanged: {fault}"),
        ),
        Ok((before, after)) => {
            let paths = differing(&before, &after, false);
            report(&run, "tree-unchanged", msg, &before, &after, &paths);
        }
    }
}

/// Reads the tree in `dir` and reports the verdict of the comparison `id`,
/// whose faults name the function `op`, of that tree with `want`.
#[track_caller]
fn compare_read(
    run: &Run<'_>,
    id: &'static str,
    op: &str,
    msg: &str,
    want: &Tree,
    dir: &Path,
    contains: bool,
) {
    match want.check().and_then(|()| read_tree(dir, true)) {
        Err(fault) => run.fault(Mode::Fatal, id, msg, format_args!("{op}: {fault}")),
        Ok(got) => report(run, id, msg, want, &got, &differing(want, &got, contains)),
    }
}

/// Reports a pass of the comparison `id` when no path differs, and otherwise
/// a failure whose record states `want` and `got` at `paths`.
#[track_caller]
fn report(run: &Run<'_>, id: &'static str, msg: &str, want: &Tree, got: &Tree, paths: &[String]) {
    if paths.is_empty() {
        run.pass(Mode::Fatal, id, msg);
        return;
    }
    run.fail(Mode::Fatal, id, msg, Record::of(want, got, paths).detail());
}
