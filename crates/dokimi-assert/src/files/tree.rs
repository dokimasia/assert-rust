// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! A tree of files, its rules and its literal.

use super::digest::sha256;
use super::entry::{CONTENT_LIMIT, Entry, Form};
use crate::json::{Json, object};
use crate::value::hex;
use std::collections::BTreeMap;
use std::fmt;

/// A tree of files: each entry at its path, relative to the tree's root.
///
/// A path is one or more names joined by `/`. A name is not empty, `.` or
/// `..`, and contains no `\` and no NUL. No entry is below a file or a link,
/// and every parent of an entry is a directory of the tree, stated or not. A
/// tree keeps its entries in the order of the bytes of their paths, and an
/// assertion ends with a fault for a tree that breaks a rule.
///
/// ```
/// use dokimi_assert::files::{self, Tree};
///
/// let tree = Tree::from([
///     ("bin/run", files::executable("#!/bin/sh\necho ok\n")),
///     ("keys", files::directory().with_mode(0o700)),
///     ("keys/id", files::text("secret\n").with_mode(0o600)),
/// ]);
/// assert_eq!(tree.len(), 3);
/// assert_eq!(tree.get("keys/id"), Some(&files::text("secret\n").with_mode(0o600)));
/// ```
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Tree {
    pub(crate) entries: BTreeMap<String, Entry>,
}

impl Tree {
    /// Returns a tree without entries.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts `entry` at `path`, and returns the entry that was there.
    pub fn insert(&mut self, path: impl Into<String>, entry: Entry) -> Option<Entry> {
        self.entries.insert(path.into(), entry)
    }

    /// Returns the entry at `path`.
    #[must_use]
    pub fn get(&self, path: &str) -> Option<&Entry> {
        self.entries.get(path)
    }

    /// Returns the number of entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns `true` for a tree without entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Returns `Err` at the first entry, in path order, that breaks a rule of
    /// a tree.
    pub(crate) fn check(&self) -> Result<(), Fault> {
        for (path, entry) in &self.entries {
            check_path(path)
                .and_then(|()| self.check_entry(path, entry))
                .map_err(|reason| Fault::at(path, reason))?;
        }
        Ok(())
    }

    fn check_entry(&self, path: &str, entry: &Entry) -> Result<(), String> {
        if let Form::Link(target) = &entry.form {
            if target.is_empty() || target.contains('\0') {
                return Err(format!(
                    "the link to {target:?} states no target of text without NUL"
                ));
            }
        }
        for parent in parents(path) {
            if let Some(above) = self.entries.get(parent) {
                if !matches!(above.form, Form::Directory) {
                    return Err(format!(
                        "the entry is below {parent:?}, which is a {}",
                        above.kind()
                    ));
                }
            }
        }
        Ok(())
    }

    /// Returns the tree with every directory that it implies, a parent of an
    /// entry that it does not state, as a directory without a mode.
    pub(crate) fn full(&self) -> Self {
        let mut out = self.clone();
        for path in self.entries.keys() {
            for parent in parents(path) {
                out.entries
                    .entry(parent.to_owned())
                    .or_insert_with(super::entry::directory);
            }
        }
        out
    }

    /// Returns the tree with `scrub` applied to the content of each file that
    /// is UTF-8 text.
    pub(crate) fn scrubbed(mut self, scrub: impl Fn(&str) -> String) -> Self {
        for entry in self.entries.values_mut() {
            if let Form::File(content) = &mut entry.form {
                if let Ok(text) = std::str::from_utf8(content) {
                    *content = scrub(text).into_bytes();
                }
            }
        }
        self
    }

    /// Returns the tree literal of the definition: the entries in path
    /// order, each file as text when its content is UTF-8 and as bytes
    /// otherwise, and by its digest and its size when the content is longer
    /// than 65,536 bytes.
    pub(crate) fn literal(&self) -> Json {
        let entries = self
            .entries
            .iter()
            .map(|(path, entry)| entry_literal(path, entry))
            .collect();
        object([("type", "tree".into()), ("entries", Json::Array(entries))])
    }
}

/// Returns the literal of the entry at `path`.
fn entry_literal(path: &str, entry: &Entry) -> Json {
    let mut members = vec![("path".to_owned(), Json::from(path))];
    match &entry.form {
        Form::File(content) if content.len() > CONTENT_LIMIT => {
            members.push(("digest".to_owned(), sha256(content).into()));
            members.push(("size".to_owned(), content.len().into()));
        }
        Form::File(content) => match std::str::from_utf8(content) {
            Ok(text) => members.push(("text".to_owned(), text.into())),
            Err(_) => members.push(("bytes".to_owned(), hex(content).into())),
        },
        Form::Directory => members.push(("directory".to_owned(), true.into())),
        Form::Link(target) => members.push(("link".to_owned(), target.as_str().into())),
    }
    if entry.is_executable() && !entry.stated {
        members.push(("executable".to_owned(), true.into()));
    }
    if entry.stated {
        members.push(("mode".to_owned(), entry.mode.into()));
    }
    Json::Object(members)
}

/// Returns `Err` with the reason for a path that breaks a rule of a path.
pub(crate) fn check_path(path: &str) -> Result<(), String> {
    for name in path.split('/') {
        if name.is_empty() || name == "." || name == ".." || name.contains(['\\', '\0']) {
            return Err(format!("the path {path:?} has the name {name:?}"));
        }
    }
    Ok(())
}

/// Returns each directory above `path`, outermost first: `a`, then `a/b`,
/// for `a/b/c`.
pub(crate) fn parents(path: &str) -> impl Iterator<Item = &str> {
    path.match_indices('/').map(|(at, _)| &path[..at])
}

impl<P: Into<String>, const N: usize> From<[(P, Entry); N]> for Tree {
    fn from(entries: [(P, Entry); N]) -> Self {
        entries.into_iter().collect()
    }
}

impl<P: Into<String>> FromIterator<(P, Entry)> for Tree {
    fn from_iter<I: IntoIterator<Item = (P, Entry)>>(entries: I) -> Self {
        Self {
            entries: entries
                .into_iter()
                .map(|(path, entry)| (path.into(), entry))
                .collect(),
        }
    }
}

/// Prints the entries in path order, each as the expression that states it,
/// such as `{"a.txt": text("a\n")}`.
impl fmt::Debug for Tree {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map().entries(&self.entries).finish()
    }
}

/// Why a call of the module cannot run: a tree that breaks a rule of a tree,
/// or an error of the file system.
#[derive(Debug)]
pub(crate) struct Fault {
    /// The path of the tree at which the fault occurred.
    pub(crate) at: Option<String>,
    /// What went wrong.
    pub(crate) reason: String,
    /// The error of the file system that caused the fault.
    pub(crate) cause: Option<std::io::Error>,
}

impl Fault {
    /// Returns the fault of `reason`.
    pub(crate) fn new(reason: impl Into<String>) -> Self {
        Self {
            at: None,
            reason: reason.into(),
            cause: None,
        }
    }

    /// Returns the fault of `reason` at `path`. The empty path is the root of
    /// a tree, which a fault does not name.
    pub(crate) fn at(path: &str, reason: impl Into<String>) -> Self {
        Self {
            at: (!path.is_empty()).then(|| path.to_owned()),
            ..Self::new(reason)
        }
    }

    /// Returns the fault with `cause` as the error that caused it.
    pub(crate) fn because(mut self, cause: std::io::Error) -> Self {
        self.cause = Some(cause);
        self
    }

    /// Returns `true` for a fault whose cause finds nothing at a path: no
    /// entry there, or a file among the names before the last one.
    pub(crate) fn finds_nothing(&self) -> bool {
        self.cause.as_ref().is_some_and(|cause| {
            matches!(
                cause.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
            )
        })
    }
}

impl fmt::Display for Fault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(at) = &self.at {
            write!(f, "{at}: ")?;
        }
        f.write_str(&self.reason)?;
        if let Some(cause) = &self.cause {
            write!(f, ": {cause}")?;
        }
        Ok(())
    }
}
