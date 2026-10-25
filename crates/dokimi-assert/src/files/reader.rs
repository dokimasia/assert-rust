// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! Reading a tree from a directory, and the entry at one path.
//!
//! A reader follows no link: a link is an entry with its target. It states
//! the permission bits of each file and each directory as the platform
//! records them.

use super::entry::{Entry, Form, execute_bit};
use super::tree::{Fault, Tree};
use std::fs::{self, Metadata};
use std::path::{Path, PathBuf};

#[cfg(unix)]
use super::entry::PERMISSIONS;

/// The permission bits that the file systems of the platform record: all
/// nine on Unix.
#[cfg(unix)]
pub(crate) const RECORDED_BITS: u32 = PERMISSIONS;

/// The permission bits that the file systems of the platform record: none
/// outside Unix. Windows keeps one bit of a file, the owner's write bit, as
/// its read-only attribute.
#[cfg(not(unix))]
pub(crate) const RECORDED_BITS: u32 = 0;

/// Returns the path of the operating system at which `path`, a path of a
/// tree, is below `dir`.
pub(crate) fn os_path(dir: &Path, path: &str) -> PathBuf {
    path.split('/')
        .filter(|name| !name.is_empty())
        .fold(dir.to_path_buf(), |at, name| at.join(name))
}

/// Returns the tree in `dir`: every entry below it, each link as a link with
/// its target, and the permission bits of each file and each directory as
/// the platform records them. With `modes` false, it states the owner's
/// execute bit of each file alone. The root may be a link to a directory.
pub(crate) fn read_tree(dir: &Path, modes: bool) -> Result<Tree, Fault> {
    let root =
        fs::metadata(dir).map_err(|cause| Fault::new("the tree cannot be read").because(cause))?;
    if !root.is_dir() {
        return Err(Fault::new("the root of the tree is no directory"));
    }
    let mut tree = Tree::new();
    walk(dir, "", modes, &mut tree)?;
    Ok(tree)
}

/// Reads the entries below the directory `under` of `dir` into `tree`.
fn walk(dir: &Path, under: &str, modes: bool, tree: &mut Tree) -> Result<(), Fault> {
    let unreadable = |cause| Fault::at(under, "the directory cannot be read").because(cause);
    for item in fs::read_dir(os_path(dir, under)).map_err(unreadable)? {
        let name = item.map_err(unreadable)?.file_name();
        let Some(name) = name.to_str() else {
            return Err(Fault::at(
                under,
                format!("the name {name:?} is no UTF-8 text"),
            ));
        };
        let path = if under.is_empty() {
            name.to_owned()
        } else {
            format!("{under}/{name}")
        };
        let entry = read_entry(&os_path(dir, &path), modes, true).map_err(|fault| Fault {
            at: Some(path.clone()),
            ..fault
        })?;
        let below = matches!(entry.form, Form::Directory);
        tree.insert(path.clone(), entry);
        if below {
            walk(dir, &path, modes, tree)?;
        }
    }
    Ok(())
}

/// Returns the entry at `path`, a path of the operating system, and `None`
/// when nothing is there. Nothing is at a path below a file. It follows no
/// link at `path`, and the platform resolves a link among the names before
/// the last one. It reads the content of a file when `content` is set.
pub(crate) fn read_path(path: &Path, content: bool) -> Result<Option<Entry>, Fault> {
    match read_entry(path, true, content) {
        Err(fault) if fault.finds_nothing() => Ok(None),
        read => read.map(Some),
    }
}

/// Returns the entry at `at`, with the permission bits that the platform
/// records: the nine bits with `modes` set, and the execute bit of a file
/// alone without.
fn read_entry(at: &Path, modes: bool, content: bool) -> Result<Entry, Fault> {
    let unreadable = |cause| Fault::new("the entry cannot be read").because(cause);
    let meta = fs::symlink_metadata(at).map_err(unreadable)?;
    let kind = meta.file_type();
    let form = if kind.is_symlink() {
        let target = fs::read_link(at).map_err(unreadable)?;
        let Some(target) = target.to_str() else {
            return Err(Fault::new(format!(
                "the target {} of the link is no UTF-8 text",
                target.display()
            )));
        };
        return Ok(super::entry::link(slashed(target)));
    } else if kind.is_dir() {
        Form::Directory
    } else if kind.is_file() {
        Form::File(if content {
            fs::read(at).map_err(unreadable)?
        } else {
            Vec::new()
        })
    } else {
        return Err(Fault::new("the entry is no file, directory or link"));
    };
    let bits = permission_bits(&meta) & RECORDED_BITS;
    let stated = modes && RECORDED_BITS != 0;
    let mode = if stated {
        bits
    } else {
        bits & execute_bit(&form)
    };
    Ok(Entry { form, mode, stated })
}

/// Returns the nine permission bits of the entry that `meta` describes.
#[cfg(unix)]
pub(crate) fn permission_bits(meta: &Metadata) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    meta.permissions().mode() & PERMISSIONS
}

/// Returns no permission bits, which the platform does not record.
#[cfg(not(unix))]
pub(crate) fn permission_bits(_: &Metadata) -> u32 {
    0
}

/// Returns the target of a link as a tree states it.
#[cfg(not(windows))]
fn slashed(target: &str) -> String {
    target.to_owned()
}

/// Returns the target of a link as a tree states it: with `/` between its
/// names, and without the prefix that Windows writes before an absolute
/// target.
#[cfg(windows)]
fn slashed(target: &str) -> String {
    target
        .strip_prefix(r"\\?\")
        .unwrap_or(target)
        .replace('\\', "/")
}
