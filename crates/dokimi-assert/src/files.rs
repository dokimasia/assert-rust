//! Trees of files for a test, and the assertions over the files that the
//! code under test reads and writes.
//!
//! A [`Tree`] states files with their content, directories and symbolic
//! links, each at a path relative to the tree's root, with `/` between the
//! names. [`text`], [`bytes`], [`executable`], [`directory`] and [`link`]
//! state its entries, and [`Entry::with_mode`] states the permission bits of
//! a file or a directory where they are part of the contract.
//!
//! ```
//! use dokimi_assert::{Collector, files};
//! use dokimi_assert::files::Tree;
//!
//! let t = Collector::new();
//! let dir = files::workspace(&t, &Tree::from([
//!     ("go.mod", files::text("module example.com/a\n")),
//!     ("a/a.go", files::text("package a\n\nfunc Old() {}\n")),
//! ]));
//! // The code under test renames Old to New in dir.
//! files::write(&t, &dir, &Tree::from([("a/a.go", files::text("package a\n\nfunc New() {}\n"))]));
//! files::contains(&t, &dir, &Tree::from([("a/a.go", files::text("package a\n\nfunc New() {}\n"))]), "the rename rewrites the declaration");
//! ```
//!
//! [`workspace`] writes a tree into a directory of the test's own, which the
//! [`Workspace`] removes when it drops. [`write()`] writes a tree over the
//! entries of a directory that exists, such as an edit between two runs of
//! the code under test. [`equal`], [`contains`] and [`unchanged`] compare the
//! tree in a directory with a wanted one. [`absent`], [`is_file`],
//! [`is_dir`], [`links_to`], [`has_content`] and [`has_mode`] check the entry
//! at one path, and [`read`] returns the text of a file.
//!
//! # Comparison rules
//!
//! A tree read from a directory follows no link, and states the permission
//! bits of each file and each directory. A comparison compares a mode only
//! where the wanted entry states one, and otherwise the owner's execute bit
//! of a file. Content compares as bytes, so a file that ends in `\r\n` does
//! not equal text that ends in `\n`.
//!
//! # Failures
//!
//! Every assertion of the module ends the test at a failure, as the golden
//! comparisons do. The record of [`equal`], [`contains`] and [`unchanged`]
//! states `want` and `got`, the trees of the entries at the first 64 paths
//! that differ, and `differences`, the number of those paths. A record states
//! a file longer than 65,536 bytes by its size and its SHA-256 digest.
//!
//! A tree that breaks a rule of a tree, an entry that is no file, directory
//! or link, and an error of the file system end the call with a fault.
//!
//! # Platforms
//!
//! Windows records no permission bits, so a tree reads no mode and no
//! execute bit there, and [`has_mode`] ends the call with a fault. A write
//! there sets the read-only attribute of each file whose mode lacks the
//! owner's write bit. A tree reads the target of a link with `/` between its
//! names on every platform.

mod compare;
mod digest;
mod entry;
mod path;
mod reader;
mod tree;
mod workspace;
mod writer;

pub use compare::{contains, equal, unchanged};
pub use entry::{Entry, bytes, directory, executable, link, text};
pub use path::{absent, has_content, has_mode, is_dir, is_file, links_to};
pub use tree::Tree;
pub use workspace::{Workspace, read, workspace, write};

pub(crate) use compare::{Record, differing};
pub(crate) use reader::read_tree;
pub(crate) use tree::{Fault, check_path};
pub(crate) use writer::update;
