// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The files vectors of the definition, run against `dokimi-assert`.
//!
//! A vector states the tree of a workspace, the arguments of a call of an
//! assertion that reads files or the subject of `tree-unchanged`, the golden
//! tree of a vector of `golden-match-tree`, and the verdict and the detail
//! of the call. The runner writes the trees of a vector into a directory of
//! the vector's own with `files::write`, makes the call on a recorder, and
//! compares the call record with the vector as JSON. It reads a golden tree
//! after an update with the standard library alone.

use crate::definition::{self, Error};
use crate::literal::{self, Malformed};
use dokimi_assert::files::{self, Tree};
use dokimi_assert::{Recorder, Value, golden};
use serde_json::{Map, Value as Json, json};
use std::fmt::{self, Write as _};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// The contract of the call of a files vector.
pub const CONTRACT: &str = "the files of the vector are as the vector states";

/// The directory below the directory of a vector that the runner writes the
/// workspace into.
const WORKSPACE: &str = "workspace";

/// The directory, relative to the working directory, against which
/// `golden::matches_tree` resolves a name.
const GOLDEN: &str = "testdata/golden";

/// One vector of the files engine.
#[derive(Debug, Clone, PartialEq)]
pub struct Vector {
    /// The id of the vector.
    pub id: String,
    /// The kind of the vector: the id of the assertion that it calls.
    pub kind: String,
    /// The vector as its file states it.
    pub raw: Json,
}

impl Vector {
    /// Returns `true` for a vector of `golden-match-tree`, which runs with its
    /// directory as the working directory of the process.
    #[must_use]
    pub fn in_working_directory(&self) -> bool {
        self.kind == "golden-match-tree"
    }

    /// Returns `true` when the platform records what the vector states. Unix
    /// records a mode and an executable file. A platform that records no
    /// permission bits skips each vector that states either, and each vector
    /// of `has-mode`.
    #[must_use]
    pub fn recorded(&self) -> bool {
        cfg!(unix) || !(self.kind == "has-mode" || states_a_mode(&self.raw))
    }
}

/// Returns `true` for a value of a vector that states a mode or an
/// executable file.
fn states_a_mode(value: &Json) -> bool {
    match value {
        Json::Object(members) => {
            members.contains_key("mode")
                || members.get("executable") == Some(&Json::Bool(true))
                || members.values().any(states_a_mode)
        }
        Json::Array(items) => items.iter().any(states_a_mode),
        _ => false,
    }
}

/// Returns every vector of the files engine, in the order of its files.
///
/// # Errors
///
/// Returns the error of a vector file that cannot be read, or that states
/// no kind or a vector without an id.
pub fn vectors() -> Result<Vec<Vector>, Error> {
    let mut found = Vec::new();
    for file in definition::vector_files("files")? {
        let stated = definition::read(&file)?;
        let Some(kind) = stated["kind"].as_str() else {
            return Err(Error::Malformed {
                file,
                what: "the file names no kind",
            });
        };
        for raw in stated["cases"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default()
        {
            let Some(id) = raw["id"].as_str() else {
                return Err(Error::Malformed {
                    file,
                    what: "a vector has no id",
                });
            };
            found.push(Vector {
                id: id.to_owned(),
                kind: kind.to_owned(),
                raw: raw.clone(),
            });
        }
    }
    Ok(found)
}

/// How the run of a files vector disagrees with the vector.
#[derive(Debug)]
#[non_exhaustive]
pub enum Disagreement {
    /// The runner calls no assertion of the vector's kind.
    Kind(String),
    /// The vector states no argument at a position, or a value of another
    /// type than the assertion takes there.
    Argument {
        /// The position of the argument.
        at: usize,
        /// The kind of value that the assertion takes there.
        expected: &'static str,
    },
    /// A tree of the vector is no tree literal of an input.
    Literal {
        /// The member of the vector that states the tree.
        member: &'static str,
        /// The literal.
        literal: Malformed,
    },
    /// A tree of the vector cannot be written.
    Write {
        /// The member of the vector that states the tree.
        member: &'static str,
        /// The fault of the write.
        fault: String,
    },
    /// A vector of `golden-match-tree` runs in another directory than the
    /// working directory.
    WorkingDirectory,
    /// The vector names no subject of files, or its subject fails.
    Subject(String),
    /// The recorder kept no call record.
    NoCallRecord,
    /// The verdict of the call record differs from the vector's.
    Verdict {
        /// The verdict that the vector expects.
        want: String,
        /// The verdict of the call record.
        got: Json,
        /// The fault of a call that ended with one.
        error: Option<String>,
    },
    /// The detail of the call record states other fields than the vector.
    DetailFields {
        /// The fields that the vector states.
        want: Vec<String>,
        /// The fields that the call record states.
        got: Vec<String>,
    },
    /// A field of the detail of the call record differs from the vector's.
    DetailField {
        /// The name of the field.
        name: String,
        /// The value that the vector states.
        want: Json,
        /// The value that the call record states.
        got: Json,
    },
    /// The golden tree after an update differs from the vector's.
    After {
        /// The tree that the vector states.
        want: Json,
        /// The tree in the golden directory.
        got: Json,
    },
    /// The golden tree after an update cannot be read.
    Unread(io::Error),
}

impl fmt::Display for Disagreement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Kind(kind) => write!(f, "the runner calls no assertion of the kind {kind:?}"),
            Self::Argument { at, expected } => {
                write!(f, "the vector states no {expected} as argument {at}")
            }
            Self::Literal { member, literal } => write!(f, "{member}: {literal}"),
            Self::Write { member, fault } => write!(f, "{member} cannot be written: {fault}"),
            Self::WorkingDirectory => f.write_str(
                "the directory of the vector is not the working directory, which golden::matches_tree resolves a name against",
            ),
            Self::Subject(what) => f.write_str(what),
            Self::NoCallRecord => f.write_str("the recorder kept no call record"),
            Self::Verdict { want, got, error } => {
                write!(f, "the call ends as {got}, and the vector expects {want:?}")?;
                if let Some(error) = error {
                    write!(f, ": {error}")?;
                }
                Ok(())
            }
            Self::DetailFields { want, got } => write!(
                f,
                "the call record states the fields {got:?}, and the vector states {want:?}"
            ),
            Self::DetailField { name, want, got } => write!(
                f,
                "the field {name} of the call record is {got}, and the vector states {want}"
            ),
            Self::After { want, got } => write!(
                f,
                "the golden tree after the call is {got}, and the vector states {want}"
            ),
            Self::Unread(error) => write!(f, "the golden tree after the call cannot be read: {error}"),
        }
    }
}

impl std::error::Error for Disagreement {}

/// Runs `vector` in `dir`, an empty directory of the vector's own, and
/// compares the call record with the vector.
///
/// A vector of `golden-match-tree` runs with `dir` as the working directory
/// of the process, because `golden::matches_tree` resolves a name against
/// it.
///
/// # Errors
///
/// Returns how the run disagrees with the vector.
pub fn run(vector: &Vector, dir: &Path) -> Result<(), Disagreement> {
    let raw = &vector.raw;
    write(dir, WORKSPACE, &raw["workspace"], "workspace")?;
    let ws = dir.join(WORKSPACE);
    let args = raw["args"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    let seat = Recorder::new();
    match vector.kind.as_str() {
        "tree-equal" => files::equal(&seat, &ws, &tree_arg(args)?, CONTRACT),
        "tree-contains" => files::contains(&seat, &ws, &tree_arg(args)?, CONTRACT),
        "tree-unchanged" => unchanged(&seat, &ws, raw)?,
        "golden-match-tree" => return matches_tree(&seat, dir, raw, args),
        "path-absent" => files::absent(&seat, path_arg(&ws, args)?, CONTRACT),
        "is-file" => files::is_file(&seat, path_arg(&ws, args)?, CONTRACT),
        "is-dir" => files::is_dir(&seat, path_arg(&ws, args)?, CONTRACT),
        "links-to" => {
            let target = text(args, 1)?;
            files::links_to(&seat, path_arg(&ws, args)?, target, CONTRACT);
        }
        "has-content" => {
            let content = content(args)?;
            files::has_content(&seat, path_arg(&ws, args)?, content, CONTRACT);
        }
        "has-mode" => {
            let mode = mode(args)?;
            files::has_mode(&seat, path_arg(&ws, args)?, mode, CONTRACT);
        }
        kind => return Err(Disagreement::Kind(kind.to_owned())),
    }
    compare(&seat, raw)
}

/// Writes the tree that `literal` states into `dir`, below the directory
/// `under`, which the write creates. A null literal writes nothing.
fn write(
    dir: &Path,
    under: &str,
    literal: &Json,
    member: &'static str,
) -> Result<(), Disagreement> {
    if literal.is_null() {
        return Ok(());
    }
    let entries =
        literal::entries(literal).map_err(|literal| Disagreement::Literal { member, literal })?;
    let mut tree = Tree::from([(under, files::directory())]);
    for (path, entry) in entries {
        tree.insert(format!("{under}/{path}"), entry);
    }
    let seat = Recorder::new();
    files::write(&seat, dir, &tree);
    if seat.failed() {
        return Err(Disagreement::Write {
            member,
            fault: seat.message(),
        });
    }
    Ok(())
}

/// Returns the wanted tree that the first argument states.
fn tree_arg(args: &[Json]) -> Result<Tree, Disagreement> {
    let literal = args.first().ok_or(Disagreement::Argument {
        at: 0,
        expected: "tree",
    })?;
    literal::tree(literal).map_err(|literal| Disagreement::Literal {
        member: "args",
        literal,
    })
}

/// Returns the text that the argument at `at` states.
fn text(args: &[Json], at: usize) -> Result<&str, Disagreement> {
    args.get(at)
        .filter(|literal| literal["type"] == "string")
        .and_then(|literal| literal["value"].as_str())
        .ok_or(Disagreement::Argument {
            at,
            expected: "string",
        })
}

/// Returns the path of the operating system that the first argument states,
/// relative to the workspace `ws`.
fn path_arg(ws: &Path, args: &[Json]) -> Result<PathBuf, Disagreement> {
    let path = text(args, 0)?;
    Ok(path
        .split('/')
        .fold(ws.to_path_buf(), |at, name| at.join(name)))
}

/// Returns the content that the second argument states, as text or as bytes.
fn content(args: &[Json]) -> Result<Vec<u8>, Disagreement> {
    match args.get(1).map(literal::decode) {
        Some(Ok(Value::String(text))) => Ok(text.into_bytes()),
        Some(Ok(Value::Bytes(bytes))) => Ok(bytes),
        _ => Err(Disagreement::Argument {
            at: 1,
            expected: "string or bytes",
        }),
    }
}

/// Returns the mode that the second argument states.
fn mode(args: &[Json]) -> Result<u32, Disagreement> {
    args.get(1)
        .filter(|literal| literal["type"] == "int")
        .and_then(|literal| literal["value"].as_u64())
        .and_then(|mode| u32::try_from(mode).ok())
        .ok_or(Disagreement::Argument {
            at: 1,
            expected: "int",
        })
}

/// Calls `files::unchanged` on `ws` with the subject that the vector names.
fn unchanged(seat: &Recorder, ws: &Path, raw: &Json) -> Result<(), Disagreement> {
    let kind = raw["subject"]["kind"].as_str().unwrap_or_default();
    let touch: fn(&Path) -> io::Result<()> = match kind {
        "leaves-files-alone" => |ws| each_file(ws, &mut |at| fs::read(at).map(drop)),
        "rewrites-files" => |ws| each_file(ws, &mut |at| fs::write(at, fs::read(at)?)),
        "writes-a-file" => |ws| fs::write(ws.join("new.txt"), "new"),
        "writes-a-large-file" => |ws| fs::write(ws.join("large.bin"), "a".repeat(65_537)),
        kind => {
            return Err(Disagreement::Subject(format!(
                "{kind:?} is no subject of files"
            )));
        }
    };
    let mut failed = None;
    files::unchanged(seat, ws, || failed = touch(ws).err(), CONTRACT);
    match failed {
        Some(error) => Err(Disagreement::Subject(format!("the subject fails: {error}"))),
        None => compare(seat, raw),
    }
}

/// Calls `visit` with the path of each file at and below `dir`, and follows
/// no link.
fn each_file(dir: &Path, visit: &mut dyn FnMut(&Path) -> io::Result<()>) -> io::Result<()> {
    for item in fs::read_dir(dir)? {
        let at = item?.path();
        let meta = fs::symlink_metadata(&at)?;
        if meta.is_dir() {
            each_file(&at, visit)?;
        } else if meta.is_file() {
            visit(&at)?;
        }
    }
    Ok(())
}

/// Calls `golden::matches_tree` with the name and the update that the vector
/// states, on the golden tree that it writes, and compares the call record
/// and the golden tree after the call with the vector.
fn matches_tree(
    seat: &Recorder,
    dir: &Path,
    raw: &Json,
    args: &[Json],
) -> Result<(), Disagreement> {
    let here = std::env::current_dir().and_then(fs::canonicalize).ok();
    if here.is_none() || here != fs::canonicalize(dir).ok() {
        return Err(Disagreement::WorkingDirectory);
    }
    let name = text(args, 0)?;
    let update = args
        .get(1)
        .filter(|literal| literal["type"] == "bool")
        .and_then(|literal| literal["value"].as_bool())
        .ok_or(Disagreement::Argument {
            at: 1,
            expected: "bool",
        })?;
    let under = format!("{GOLDEN}/{name}");
    write(dir, &under, &raw["golden"], "golden")?;
    golden::matches_tree(seat, name, dir.join(WORKSPACE), update, &[]);
    compare(seat, raw)?;
    let Some(want) = raw.get("after") else {
        return Ok(());
    };
    let got = tree_literal(&dir.join(under)).map_err(Disagreement::Unread)?;
    if got != *want {
        return Err(Disagreement::After {
            want: want.clone(),
            got,
        });
    }
    Ok(())
}

/// Returns the tree literal of the tree in `dir`, read without its modes:
/// each file with its content and its execute bit, each directory, and each
/// link with its target.
fn tree_literal(dir: &Path) -> io::Result<Json> {
    let mut entries = Vec::new();
    read_entries(dir, "", &mut entries)?;
    entries.sort_by(|a, b| a["path"].as_str().cmp(&b["path"].as_str()));
    Ok(json!({"type": "tree", "entries": entries}))
}

fn read_entries(dir: &Path, under: &str, entries: &mut Vec<Json>) -> io::Result<()> {
    for item in fs::read_dir(dir.join(under))? {
        let name = item?.file_name().to_string_lossy().into_owned();
        let path = if under.is_empty() {
            name
        } else {
            format!("{under}/{name}")
        };
        let at = dir.join(&path);
        let meta = fs::symlink_metadata(&at)?;
        let mut entry = Map::new();
        entry.insert("path".to_owned(), Json::from(path.clone()));
        if meta.is_symlink() {
            let target = fs::read_link(&at)?.to_string_lossy().replace('\\', "/");
            entry.insert("link".to_owned(), Json::from(target));
        } else if meta.is_dir() {
            entry.insert("directory".to_owned(), Json::Bool(true));
            read_entries(dir, &path, entries)?;
        } else {
            let content = fs::read(&at)?;
            match String::from_utf8(content) {
                Ok(text) => entry.insert("text".to_owned(), Json::from(text)),
                Err(bytes) => entry.insert("bytes".to_owned(), Json::from(hex(bytes.as_bytes()))),
            };
            if executable(&meta) {
                entry.insert("executable".to_owned(), Json::Bool(true));
            }
        }
        entries.push(Json::Object(entry));
    }
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut text, byte| {
        // Writing to a String cannot fail.
        let _ = write!(text, "{byte:02x}");
        text
    })
}

/// Returns `true` for a file that its owner may execute.
#[cfg(unix)]
fn executable(meta: &fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    meta.permissions().mode() & 0o100 != 0
}

/// Returns `false`: the platform records no execute bit.
#[cfg(not(unix))]
fn executable(_: &fs::Metadata) -> bool {
    false
}

/// Compares the one call record of `seat` with the verdict and the detail
/// that the vector states.
fn compare(seat: &Recorder, raw: &Json) -> Result<(), Disagreement> {
    let line = seat
        .records()
        .into_iter()
        .next()
        .ok_or(Disagreement::NoCallRecord)?;
    let record: Json = serde_json::from_str(&line).map_err(|_| Disagreement::NoCallRecord)?;
    let want = raw["expect"].as_str().unwrap_or_default();
    if record["verdict"] != want {
        return Err(Disagreement::Verdict {
            want: want.to_owned(),
            got: record["verdict"].clone(),
            error: record["error"].as_str().map(str::to_owned),
        });
    }
    let stated = raw["detail"].as_object().cloned().unwrap_or_default();
    let detail = record["detail"].as_object().cloned().unwrap_or_default();
    let names = |members: &Map<String, Json>| members.keys().cloned().collect::<Vec<_>>();
    if names(&stated) != names(&detail) {
        return Err(Disagreement::DetailFields {
            want: names(&stated),
            got: names(&detail),
        });
    }
    for (name, want) in stated {
        if detail[&name] != want {
            return Err(Disagreement::DetailField {
                got: detail[&name].clone(),
                name,
                want,
            });
        }
    }
    Ok(())
}
