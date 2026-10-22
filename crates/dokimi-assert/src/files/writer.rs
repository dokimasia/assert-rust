// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! Writing a tree into a directory: the directory of a workspace, a
//! directory that exists, and a golden directory that an update makes equal
//! a tree.
//!
//! A write never follows a link. It creates each entry with a call that
//! refuses an entry that is there, in path order, so each parent is a
//! directory that the write created or read before it writes below the
//! parent. It sets the mode of a file after it writes the file, and the mode
//! of a directory after every entry below it, so the umask cannot change a
//! mode, and the mode of a directory cannot stop a write below it.

use super::compare::differs;
use super::entry::{Entry, Form};
use super::reader::{os_path, permission_bits, read_tree};
use super::tree::{Fault, Tree};
use std::collections::BTreeSet;
use std::fs::{self, Metadata};
use std::io::{self, ErrorKind, Write as _};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

/// The permission bits with which a write creates a file, before it sets
/// the file's mode.
const CREATE_FILE: u32 = 0o600;

/// The permission bits with which a write creates a directory, before it
/// sets the directory's mode.
const CREATE_DIRECTORY: u32 = 0o700;

/// The mode of a file that states none.
const FILE_MODE: u32 = 0o644;

/// The mode of an executable file and of a directory that state none.
const EXECUTABLE_MODE: u32 = 0o755;

/// The permission bit that lets the owner write a file. Windows keeps it as
/// the file's read-only attribute.
#[cfg(not(unix))]
const OWNER_WRITE: u32 = 0o200;

/// One operation of a write, at a path of the tree.
enum Step {
    /// Creates a file of the content, and gives it the mode.
    File {
        path: String,
        content: Vec<u8>,
        mode: u32,
    },
    /// Creates a directory.
    Directory { path: String },
    /// Creates a link to the target.
    Link { path: String, target: String },
    /// Writes the content and then the mode over the file at the path.
    Rewrite {
        path: String,
        content: Vec<u8>,
        mode: u32,
    },
    /// Removes the link at the path and creates a link to the target.
    Relink { path: String, target: String },
    /// Writes the file into a temporary file beside the path, and renames
    /// the temporary file over the path.
    Swap {
        path: String,
        content: Vec<u8>,
        mode: u32,
    },
    /// Removes the entry at the path, a directory with everything in it.
    Remove { path: String },
    /// Gives the directory at the path its mode.
    Mode { path: String, mode: u32 },
}

/// Returns the mode that a write sets on the file or the directory `entry`:
/// its stated mode, and where it states none 0o644, or 0o755 for an
/// executable file and a directory.
fn mode_of(entry: &Entry) -> u32 {
    if entry.stated {
        entry.mode
    } else if entry.is_executable() || matches!(entry.form, Form::Directory) {
        EXECUTABLE_MODE
    } else {
        FILE_MODE
    }
}

/// Writes `tree` into `dir`, a directory that exists, as a workspace does.
/// It creates every entry and writes over none: a file of its content, a
/// directory, and a link of its target, inside or outside `dir`.
pub(crate) fn create(dir: &Path, tree: &Tree) -> Result<(), Fault> {
    tree.check()?;
    opens(dir)?;
    apply(dir, creating(&tree.full(), &BTreeSet::new(), Vec::new()))
}

/// Writes `tree` into `dir`, a directory that exists, as `files::write`
/// does. Where nothing is at a path of the tree, it creates the entry as
/// [`create`] does. A file replaces the content of the file at its path,
/// also of a file that its owner may not write, and a link replaces the
/// target of the link at its path. A directory keeps its entries. Each entry
/// that the tree states gets its mode, and a parent that the tree implies
/// keeps its mode where it exists.
///
/// It reads the entry at each path of the tree, and at each parent of one,
/// before it writes anything, and refuses an entry of another kind than the
/// tree states and a path that the file system maps to an entry at an
/// earlier path.
pub(crate) fn overwrite(dir: &Path, tree: &Tree) -> Result<(), Fault> {
    tree.check()?;
    opens(dir)?;
    let full = tree.full();
    let mut there = BTreeSet::new();
    let mut read: Vec<(&str, Metadata)> = Vec::new();
    let mut replaced = Vec::new();
    let mut modes = Vec::new();
    for (path, entry) in &full.entries {
        let meta = match fs::symlink_metadata(os_path(dir, path)) {
            Err(cause) if cause.kind() == ErrorKind::NotFound => continue,
            meta => {
                meta.map_err(|cause| Fault::at(path, "the entry cannot be read").because(cause))?
            }
        };
        refusal(&meta, entry, &read).map_err(|reason| Fault::at(path, reason))?;
        match &entry.form {
            Form::File(content) => replaced.push(Step::Rewrite {
                path: path.clone(),
                content: content.clone(),
                mode: mode_of(entry),
            }),
            Form::Link(target) => replaced.push(Step::Relink {
                path: path.clone(),
                target: target.clone(),
            }),
            Form::Directory if tree.entries.contains_key(path) => modes.push(Step::Mode {
                path: path.clone(),
                mode: mode_of(entry),
            }),
            Form::Directory => {}
        }
        there.insert(path.clone());
        read.push((path, meta));
    }
    let mut steps = creating(&full, &there, replaced);
    steps.extend(modes.into_iter().rev());
    apply(dir, steps)
}

/// Returns why a write refuses the entry that `meta` describes where the
/// tree states `entry`: an entry that is no file, directory or link, one of
/// another kind, and one that is the entry at an earlier path in `read`.
fn refusal(meta: &Metadata, entry: &Entry, read: &[(&str, Metadata)]) -> Result<(), String> {
    let kind = meta.file_type();
    let found = if kind.is_symlink() {
        "link"
    } else if kind.is_dir() {
        "directory"
    } else if kind.is_file() {
        "file"
    } else {
        return Err("the entry is no file, directory or link".to_owned());
    };
    if found != entry.kind() {
        return Err(format!(
            "the entry is a {found}, and the tree states a {}",
            entry.kind()
        ));
    }
    match read.iter().find(|(_, other)| same_entry(meta, other)) {
        Some((other, _)) => Err(format!(
            "the file system maps the path to the entry at {other:?}"
        )),
        None => Ok(()),
    }
}

/// Makes `dir` equal `tree`, as an update of a golden tree does. It creates
/// `dir` and its parents when they are missing. It removes each entry that
/// the tree lacks or states in another form, and writes each entry that
/// `dir` then lacks, as [`create`] writes it. It compares no mode, and
/// writes each file and each directory with the mode of an entry that
/// states none.
///
/// It removes a link, and never the entry that the link points to. A file
/// that the tree states with another content or another execute bit is
/// written into a temporary file beside it, which the update renames over
/// the file once the write succeeds.
pub(crate) fn update(dir: &Path, tree: &Tree) -> Result<(), Fault> {
    tree.check()?;
    fs::create_dir_all(dir)
        .map_err(|cause| Fault::new("the directory cannot be created").because(cause))?;
    let current = read_tree(dir, false)?;
    let wanted: Tree = tree
        .full()
        .entries
        .iter()
        .map(|(path, entry)| (path.clone(), entry.as_compared(None)))
        .collect();
    let mut kept = BTreeSet::new();
    let mut steps = Vec::new();
    for (path, now) in current.entries.iter().rev() {
        match wanted.entries.get(path) {
            Some(want) if !differs(want, Some(now)) => {
                kept.insert(path.clone());
            }
            Some(
                want @ Entry {
                    form: Form::File(content),
                    ..
                },
            ) if matches!(now.form, Form::File(_)) => {
                kept.insert(path.clone());
                steps.push(Step::Swap {
                    path: path.clone(),
                    content: content.clone(),
                    mode: mode_of(want),
                });
            }
            _ => steps.push(Step::Remove { path: path.clone() }),
        }
    }
    apply(dir, creating(&wanted, &kept, steps))
}

/// Returns the steps `before`, then the steps that create each entry of
/// `full` that `kept` lacks, in path order, and then the steps that give
/// each directory that they create its mode, deepest first.
fn creating(full: &Tree, kept: &BTreeSet<String>, before: Vec<Step>) -> Vec<Step> {
    let mut steps = before;
    let mut modes = Vec::new();
    for (path, entry) in full
        .entries
        .iter()
        .filter(|(path, _)| !kept.contains(*path))
    {
        let path = path.clone();
        match &entry.form {
            Form::File(content) => steps.push(Step::File {
                path,
                content: content.clone(),
                mode: mode_of(entry),
            }),
            Form::Directory => {
                steps.push(Step::Directory { path: path.clone() });
                modes.push(Step::Mode {
                    path,
                    mode: mode_of(entry),
                });
            }
            Form::Link(target) => steps.push(Step::Link {
                path,
                target: target.clone(),
            }),
        }
    }
    steps.extend(modes.into_iter().rev());
    steps
}

/// Returns `Err` for a `dir` that is no directory that opens.
fn opens(dir: &Path) -> Result<(), Fault> {
    fs::read_dir(dir)
        .map(drop)
        .map_err(|cause| Fault::new("the directory cannot be opened").because(cause))
}

/// Runs `steps` on `dir`, in order, and returns a fault at the path of the
/// first step that fails.
fn apply(dir: &Path, steps: Vec<Step>) -> Result<(), Fault> {
    for step in steps {
        let (path, done) = run(dir, step);
        done.map_err(|cause| Fault::at(&path, "the entry cannot be written").because(cause))?;
    }
    Ok(())
}

/// Runs `step` on `dir`, and returns its path and its outcome.
fn run(dir: &Path, step: Step) -> (String, io::Result<()>) {
    match step {
        Step::File {
            path,
            content,
            mode,
        } => {
            let done = write_new(&os_path(dir, &path), &content, mode);
            (path, done)
        }
        Step::Directory { path } => {
            let done = create_dir(&os_path(dir, &path));
            (path, done)
        }
        Step::Link { path, target } => {
            let done = symlink(&target, &os_path(dir, &path));
            (path, done)
        }
        Step::Rewrite {
            path,
            content,
            mode,
        } => {
            let at = os_path(dir, &path);
            let done = set_mode(&at, CREATE_FILE, false)
                .and_then(|()| fs::write(&at, &content))
                .and_then(|()| set_mode(&at, mode, false));
            (path, done)
        }
        Step::Relink { path, target } => {
            let at = os_path(dir, &path);
            let done = remove_link(&at).and_then(|()| symlink(&target, &at));
            (path, done)
        }
        Step::Swap {
            path,
            content,
            mode,
        } => {
            let at = os_path(dir, &path);
            let temp = at.with_file_name(temporary_name());
            let done = write_new(&temp, &content, mode).and_then(|()| fs::rename(&temp, &at));
            if done.is_err() {
                // A temporary file that the failed write left is no entry of the tree.
                let _ = fs::remove_file(&temp);
            }
            (path, done)
        }
        Step::Remove { path } => {
            let done = remove(&os_path(dir, &path));
            (path, done)
        }
        Step::Mode { path, mode } => {
            let done = set_mode(&os_path(dir, &path), mode, true);
            (path, done)
        }
    }
}

/// Returns the name of a temporary file that an update writes beside a file
/// that it replaces, which no other temporary file of the process has.
fn temporary_name() -> String {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    format!(
        ".dokimi-update-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}

/// Creates the file at `at` with `content`, refusing an entry that is
/// there, and gives it `mode`.
fn write_new(at: &Path, content: &[u8], mode: u32) -> io::Result<()> {
    open_new(at)?.write_all(content)?;
    set_mode(at, mode, false)
}

/// Removes the entry at `at`: a directory with everything in it, and a file
/// or a link itself.
fn remove(at: &Path) -> io::Result<()> {
    if fs::symlink_metadata(at)?.is_dir() {
        fs::remove_dir_all(at)
    } else {
        remove_link(at)
    }
}

/// Gives the owner of each directory at and below `path` the permission to
/// read, write and search it, so that every entry below it can be removed,
/// also below a directory whose mode forbids it. It follows no link, and
/// reports nothing.
pub(crate) fn unlock(path: &Path) {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return;
    };
    if meta.is_dir() {
        let _ = set_mode(path, permission_bits(&meta) | CREATE_DIRECTORY, true);
        for item in fs::read_dir(path).into_iter().flatten().flatten() {
            unlock(&item.path());
        }
    } else {
        unlock_file(path, &meta);
    }
}

/// Creates the directory at `at`.
#[cfg(unix)]
fn create_dir(at: &Path) -> io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    fs::DirBuilder::new().mode(CREATE_DIRECTORY).create(at)
}

/// Creates the directory at `at`.
#[cfg(not(unix))]
fn create_dir(at: &Path) -> io::Result<()> {
    fs::create_dir(at)
}

/// Opens a new file at `at` for writing, refusing an entry that is there.
#[cfg(unix)]
fn open_new(at: &Path) -> io::Result<fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(CREATE_FILE)
        .open(at)
}

/// Opens a new file at `at` for writing, refusing an entry that is there.
#[cfg(not(unix))]
fn open_new(at: &Path) -> io::Result<fs::File> {
    fs::OpenOptions::new().write(true).create_new(true).open(at)
}

/// Gives the file or the directory at `at` the permission bits of `mode`.
#[cfg(unix)]
fn set_mode(at: &Path, mode: u32, _directory: bool) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(at, fs::Permissions::from_mode(mode))
}

/// Gives the file at `at` the read-only attribute when `mode` lacks the
/// owner's write bit. A directory gets nothing, because Windows does not
/// honour the attribute of a directory.
#[cfg(not(unix))]
fn set_mode(at: &Path, mode: u32, directory: bool) -> io::Result<()> {
    if directory {
        return Ok(());
    }
    let mut permissions = fs::metadata(at)?.permissions();
    permissions.set_readonly(mode & OWNER_WRITE == 0);
    fs::set_permissions(at, permissions)
}

/// Leaves a file as it is. Its directory decides whether it can be removed.
#[cfg(unix)]
fn unlock_file(_: &Path, _: &Metadata) {}

/// Clears the read-only attribute of a file, so that it can be removed.
#[cfg(not(unix))]
fn unlock_file(at: &Path, meta: &Metadata) {
    let mut permissions = meta.permissions();
    if meta.is_file() && permissions.readonly() {
        permissions.set_readonly(false);
        let _ = fs::set_permissions(at, permissions);
    }
}

/// Returns `true` when `a` and `b` describe one entry of the file system.
#[cfg(unix)]
fn same_entry(a: &Metadata, b: &Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    a.dev() == b.dev() && a.ino() == b.ino()
}

/// Returns `false`: stable Rust reads no identity of an entry outside Unix.
#[cfg(not(unix))]
fn same_entry(_: &Metadata, _: &Metadata) -> bool {
    false
}

/// Creates a link at `at` to `target`.
#[cfg(unix)]
fn symlink(target: &str, at: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(target, at)
}

/// Creates a link at `at` to `target`: a link to a directory when the target
/// resolves to a directory, and a link to a file otherwise.
#[cfg(windows)]
fn symlink(target: &str, at: &Path) -> io::Result<()> {
    let native = target.replace('/', "\\");
    let resolved = at.parent().map(|parent| parent.join(&native));
    if resolved.is_some_and(|resolved| resolved.is_dir()) {
        std::os::windows::fs::symlink_dir(native, at)
    } else {
        std::os::windows::fs::symlink_file(native, at)
    }
}

/// Refuses a link, which the platform does not have.
#[cfg(not(any(unix, windows)))]
fn symlink(_: &str, _: &Path) -> io::Result<()> {
    Err(io::Error::new(
        ErrorKind::Unsupported,
        "the platform has no symbolic links",
    ))
}

/// Removes the file or the link at `at`, and never the entry that a link
/// points to.
#[cfg(not(windows))]
fn remove_link(at: &Path) -> io::Result<()> {
    fs::remove_file(at)
}

/// Removes the file or the link at `at`, and never the entry that a link
/// points to. Windows removes a link to a directory as a directory.
#[cfg(windows)]
fn remove_link(at: &Path) -> io::Result<()> {
    use std::os::windows::fs::FileTypeExt;
    if fs::symlink_metadata(at)?.file_type().is_symlink_dir() {
        fs::remove_dir(at)
    } else {
        fs::remove_file(at)
    }
}
