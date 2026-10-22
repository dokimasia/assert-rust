// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The workspace of a test, and the helpers that write and read the files of
//! a directory that exists.

use super::entry::Form;
use super::reader::read_path;
use super::tree::{Fault, Tree};
use super::writer::{create, overwrite, unlock};
use crate::seat::Seat;
use crate::verdict;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// A directory of a test's own, below the system's temporary directory, with
/// a tree written into it.
///
/// The drop of the workspace removes the directory with everything in it,
/// also below a directory whose mode forbids the owner to write it, as
/// `tempfile::TempDir` removes its directory.
#[derive(Debug)]
pub struct Workspace {
    path: PathBuf,
}

impl Workspace {
    /// Returns the path of the directory.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl AsRef<Path> for Workspace {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        unlock(&self.path);
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// Writes `tree` into a new directory of the test's own, and returns the
/// directory.
///
/// ```
/// use dokimi_assert::{Collector, files};
/// use dokimi_assert::files::Tree;
///
/// let t = Collector::new();
/// let dir = files::workspace(&t, &Tree::from([
///     ("go.mod", files::text("module example.com/a\n")),
///     ("keys/id", files::text("secret\n").with_mode(0o600)),
/// ]));
/// files::has_content(&t, dir.path().join("go.mod"), "module example.com/a\n", "go.mod is written");
/// ```
///
/// It creates every entry and writes over none, and it never follows a
/// link: it writes a link's target as the tree states it, inside or outside
/// the directory. Each file and each directory gets its stated mode. Where
/// the tree states none, a file gets 0o644, and an executable file and a
/// directory get 0o755. The workspace sets each mode after it writes the
/// entry, and the mode of a directory after every entry below it, so the
/// umask cannot change a mode. On Windows only the owner's write bit of a
/// file's mode takes effect, as the file's read-only attribute.
///
/// A tree that breaks a rule of a tree ends the call with a fault before
/// anything is written, and so does an entry that the file system cannot
/// store as stated, such as a link where the platform refuses one.
#[track_caller]
pub fn workspace(seat: &dyn Seat, tree: &Tree) -> Workspace {
    seat.helper();
    let created =
        new_dir().map_err(|cause| Fault::new("the directory cannot be created").because(cause));
    let workspace = Workspace {
        path: created
            .as_ref()
            .map_or_else(|_| PathBuf::new(), Clone::clone),
    };
    if let Err(fault) = created.and_then(|dir| create(&dir, tree)) {
        verdict::end(seat, format_args!("files::workspace: {fault}"));
    }
    workspace
}

/// Creates a directory below the system's temporary directory, under a name
/// that no other directory of a workspace has.
fn new_dir() -> io::Result<PathBuf> {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.subsec_nanos());
    let path = std::env::temp_dir().join(format!(
        "dokimi-files-{}-{}-{nanos}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&path).map(|()| path)
}

/// Writes `tree` into `dir`, a directory that exists.
///
/// ```
/// use dokimi_assert::{Collector, files};
/// use dokimi_assert::files::Tree;
///
/// let t = Collector::new();
/// let dir = files::workspace(&t, &Tree::from([("api/store.gen.go", files::text("package api\n"))]));
/// files::write(&t, &dir, &Tree::from([("api/store.gen.go", files::text("package api // edited\n"))]));
/// files::has_content(&t, dir.path().join("api/store.gen.go"), "package api // edited\n", "the edit is written");
/// ```
///
/// Where nothing is at a path of the tree, it creates the entry as
/// [`workspace`] does. A file replaces the content of the file at its path,
/// a link replaces the target of the link at its path, and a directory keeps
/// its entries. Each entry that the tree states gets its stated mode, and
/// where it states none 0o644, or 0o755 for an executable file and a
/// directory. A parent that the tree implies keeps its mode when it exists,
/// and gets 0o755 when the write creates it. Every other entry of `dir`
/// keeps its content and its mode. The write never follows a link: it
/// replaces a link by removing the link itself.
///
/// A tree that breaks a rule of a tree, a `dir` that is no directory, an
/// entry of another kind than the tree states, and a path that the file
/// system maps to an entry at another path of the tree each end the call
/// with a fault before anything is written. A link is another kind than a
/// directory, a link to a directory included. An error of the file system
/// ends the call with a fault at the path of its entry.
#[track_caller]
pub fn write(seat: &dyn Seat, dir: impl AsRef<Path>, tree: &Tree) {
    seat.helper();
    if let Err(fault) = overwrite(dir.as_ref(), tree) {
        verdict::end(seat, format_args!("files::write: {fault}"));
    }
}

/// Returns the text of the file at `path`, so that a test composes it with
/// the text assertions.
///
/// ```
/// use dokimi_assert::{Collector, check, files};
/// use dokimi_assert::files::Tree;
///
/// let t = Collector::new();
/// let dir = files::workspace(&t, &Tree::from([("VERSION", files::text("version 2\n"))]));
/// check::has_prefix(&t, &files::read(&t, dir.path().join("VERSION")), "version 2", "the file states the new version");
/// ```
///
/// No file at `path`, a link to a file included, a file that cannot be
/// read, and a file that is no UTF-8 text end the call with a fault. It
/// returns the empty string then, on a seat that keeps the fault.
#[track_caller]
pub fn read(seat: &dyn Seat, path: impl AsRef<Path>) -> String {
    seat.helper();
    let path = path.as_ref();
    let read = read_path(path, true).and_then(|entry| match entry.map(|entry| entry.form) {
        Some(Form::File(content)) => String::from_utf8(content)
            .map_err(|_| Fault::new(format!("the file at {} is no UTF-8 text", path.display()))),
        _ => Err(Fault::new(format!(
            "no file is at the path {}",
            path.display()
        ))),
    });
    read.unwrap_or_else(|fault| {
        verdict::end(seat, format_args!("files::read: {fault}"));
        String::new()
    })
}
