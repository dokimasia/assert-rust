//! The entries of a tree: files, directories and symbolic links.

use super::digest::sha256;
use std::fmt;

/// The nine permission bits.
pub(crate) const PERMISSIONS: u32 = 0o777;

/// The permission bit that lets the owner execute a file.
pub(crate) const OWNER_EXECUTE: u32 = 0o100;

/// The longest content that a record and a sentence state in full, in bytes.
/// A longer content is stated by its size and its digest.
pub(crate) const CONTENT_LIMIT: usize = 65_536;

/// The kind of an entry, with what an entry of the kind contains.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum Form {
    /// A file, with its content.
    File(Vec<u8>),
    /// A directory.
    Directory,
    /// A symbolic link, with its target.
    Link(String),
}

/// A file, a directory or a symbolic link of a [`Tree`](super::Tree).
///
/// [`text`], [`bytes`], [`executable`], [`directory`] and [`link`] return an
/// entry, and [`Entry::with_mode`] states its permission bits. An entry
/// prints as the Rust expression that states it, such as
/// `text("secret\n").with_mode(0o600)`.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Entry {
    pub(crate) form: Form,
    /// The nine permission bits when `stated` is set. Otherwise the owner's
    /// execute bit of a file that its owner may execute, and 0.
    pub(crate) mode: u32,
    /// Whether `mode` states all nine permission bits.
    pub(crate) stated: bool,
}

/// Returns a file whose content is `text`.
#[must_use]
pub fn text(text: impl Into<String>) -> Entry {
    file(text.into().into_bytes(), 0)
}

/// Returns a file whose content is `content`.
#[must_use]
pub fn bytes(content: impl Into<Vec<u8>>) -> Entry {
    file(content.into(), 0)
}

/// Returns a file whose content is `text`, which its owner may execute.
#[must_use]
pub fn executable(text: impl Into<String>) -> Entry {
    file(text.into().into_bytes(), OWNER_EXECUTE)
}

/// Returns a directory.
///
/// A tree needs a directory entry only for an empty directory or for a
/// directory whose mode it states, because every parent of an entry is a
/// directory of the tree.
#[must_use]
pub fn directory() -> Entry {
    Entry {
        form: Form::Directory,
        mode: 0,
        stated: false,
    }
}

/// Returns a symbolic link to `target`.
///
/// The target is text that no rule of a path limits, inside or outside the
/// tree, and nothing follows it. A tree reads a target with `/` between its
/// names on every platform, so a target separates its names with `/`.
#[must_use]
pub fn link(target: impl Into<String>) -> Entry {
    Entry {
        form: Form::Link(target.into()),
        mode: 0,
        stated: false,
    }
}

fn file(content: Vec<u8>, mode: u32) -> Entry {
    Entry {
        form: Form::File(content),
        mode,
        stated: false,
    }
}

impl Entry {
    /// Returns the entry with the nine permission bits of `mode`, which a
    /// workspace sets and a comparison compares. The mode of a file states
    /// whether its owner may execute it.
    ///
    /// ```
    /// use dokimi_assert::files;
    ///
    /// let key = files::text("secret\n").with_mode(0o600);
    /// assert_eq!(format!("{key:?}"), r#"text("secret\n").with_mode(0o600)"#);
    /// ```
    ///
    /// # Panics
    ///
    /// Panics for a mode with a bit beyond the nine permission bits, and for
    /// a link, which has no mode.
    #[must_use]
    pub fn with_mode(self, mode: u32) -> Self {
        assert!(
            mode & !PERMISSIONS == 0,
            "files::Entry::with_mode({mode:#o}) states a bit beyond the nine permission bits"
        );
        assert!(
            !matches!(self.form, Form::Link(_)),
            "files::Entry::with_mode({mode:#o}) states the mode of a link, which has no mode"
        );
        Self {
            mode,
            stated: true,
            ..self
        }
    }

    /// Returns `true` for a file that its owner may execute.
    pub(crate) fn is_executable(&self) -> bool {
        matches!(self.form, Form::File(_)) && self.mode & OWNER_EXECUTE != 0
    }

    /// Returns the kind of the entry as a record states it.
    pub(crate) fn kind(&self) -> &'static str {
        match self.form {
            Form::File(_) => "file",
            Form::Directory => "directory",
            Form::Link(_) => "link",
        }
    }

    /// Returns the entry as a comparison reads it where the wanted entry
    /// `want` is: with its mode where `want` states one, and with the
    /// execute bit of a file alone where it does not.
    pub(crate) fn as_compared(&self, want: Option<&Entry>) -> Self {
        let stated = self.stated && want.is_some_and(|want| want.stated);
        let mode = if stated {
            self.mode
        } else {
            self.mode & execute_bit(&self.form)
        };
        Self {
            form: self.form.clone(),
            mode,
            stated,
        }
    }
}

/// Returns the permission bit that an entry of `form` states without a
/// mode: the execute bit of a file, and none of any other entry.
pub(crate) fn execute_bit(form: &Form) -> u32 {
    if matches!(form, Form::File(_)) {
        OWNER_EXECUTE
    } else {
        0
    }
}

/// Prints the Rust expression that states the entry. A content longer than
/// 65,536 bytes prints as its size and its digest.
impl fmt::Debug for Entry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.form {
            Form::File(content) => {
                let name = match (
                    self.is_executable() && !self.stated,
                    std::str::from_utf8(content),
                ) {
                    (true, _) => "executable",
                    (false, Ok(_)) => "text",
                    (false, Err(_)) => "bytes",
                };
                write!(f, "{name}(")?;
                if content.len() > CONTENT_LIMIT {
                    write!(f, "<{} bytes, {}>", content.len(), sha256(content))?;
                } else if let Ok(text) = std::str::from_utf8(content) {
                    write!(f, "{text:?}")?;
                } else {
                    write!(f, "b\"{}\"", content.escape_ascii())?;
                }
                f.write_str(")")?;
            }
            Form::Directory => f.write_str("directory()")?,
            Form::Link(target) => write!(f, "link({target:?})")?,
        }
        if self.stated {
            write!(f, ".with_mode({:#o})", self.mode)?;
        }
        Ok(())
    }
}
