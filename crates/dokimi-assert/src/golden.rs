//! Comparison against recorded output.
//!
//! A golden file contains the output that a test compares with, so a change
//! of rendering shows as a difference rather than as a rewritten assertion.
//! Each comparison takes the decision to update as an argument, which
//! [`should_update`] reads from `DOKIMI_ASSERT_UPDATE_GOLDEN`. Read the
//! difference before an update: an update accepts whatever the code now does.
//!
//! ```no_run
//! use dokimi_assert::{Collector, golden};
//!
//! let t = Collector::new();
//! let report = "total: 3\n";
//! golden::matches(&t, "report.txt", report, golden::should_update(), &[golden::scrub_timestamps()]);
//! ```
//!
//! Each comparison states its own contract, which names the golden file. A
//! failure states the golden content as `want`, null for a missing file, and
//! the output as `got`, both scrubbed. A golden file that cannot be read or
//! written ends the call with a fault.

use crate::files::{Fault, Record, check_path, differing, read_tree, update as update_tree};
use crate::json::{self, Json};
use crate::seat::Seat;
use crate::value::Value;
use crate::verdict::{Mode, Run};
use regex::Regex;
use std::fmt::{self, Debug};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};

/// The variable that turns rewriting on.
pub const UPDATE_ENV: &str = "DOKIMI_ASSERT_UPDATE_GOLDEN";

/// The directory that [`matches()`] resolves a name against, relative to the
/// package root, where `cargo test` runs a test.
pub const GOLDEN_DIR: &str = "testdata/golden";

/// The deepest nesting of a golden JSON document that the reader accepts.
const JSON_DEPTH: usize = 512;

/// A replacement that a comparison applies to both sides before it compares
/// them.
///
/// A scrubber removes the parts of an output that change between runs, such
/// as a timestamp, so the comparison reads only the parts that should be
/// stable. A test writes a scrubber of its own from any function of text:
///
/// ```
/// use dokimi_assert::golden::Scrubber;
///
/// let ports = Scrubber::new(|text| text.replace(":8080", ":PORT"));
/// assert_eq!(ports.apply("localhost:8080"), "localhost:PORT");
/// ```
#[derive(Clone)]
pub struct Scrubber(Arc<dyn Fn(&str) -> String + Send + Sync>);

impl Scrubber {
    /// Returns the scrubber that replaces a text by what `replace` returns.
    pub fn new(replace: impl Fn(&str) -> String + Send + Sync + 'static) -> Self {
        Self(Arc::new(replace))
    }

    /// Returns `text` with the replacement applied.
    #[must_use]
    pub fn apply(&self, text: &str) -> String {
        (self.0)(text)
    }

    fn pattern(pattern: &'static LazyLock<Regex>, replacement: &'static str) -> Self {
        Self::new(move |text| pattern.replace_all(text, replacement).into_owned())
    }
}

impl Debug for Scrubber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Scrubber").finish_non_exhaustive()
    }
}

static TIMESTAMP: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[0-9]{4}-[0-9]{2}-[0-9]{2}[Tt ][0-9]{2}:[0-9]{2}:[0-9]{2}(?:\.[0-9]+)?(?:[Zz]|[+-][0-9]{2}:?[0-9]{2})?")
        .expect("the timestamp pattern compiles")
});

static HASH: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b[0-9a-fA-F]{32,128}\b").expect("the hash pattern compiles"));

static RUN_ID: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\brun_[0-9a-z]{16}\b").expect("the run id pattern compiles"));

/// Returns the scrubber that replaces ISO 8601 and RFC 3339 timestamps, with
/// or without fractional seconds and with a zone offset or `Z`, by
/// `SCRUBBED_TIMESTAMP`.
#[must_use]
pub fn scrub_timestamps() -> Scrubber {
    Scrubber::pattern(&TIMESTAMP, "SCRUBBED_TIMESTAMP")
}

/// Returns the scrubber that replaces hexadecimal digests of 32 to 128
/// digits, MD5 through SHA-512, by `SCRUBBED_HASH`.
#[must_use]
pub fn scrub_hashes() -> Scrubber {
    Scrubber::pattern(&HASH, "SCRUBBED_HASH")
}

/// Returns the scrubber that replaces identifiers of the form `run_` and
/// sixteen lowercase letters or digits by `SCRUBBED_RUN_ID`.
#[must_use]
pub fn scrub_run_ids() -> Scrubber {
    Scrubber::pattern(&RUN_ID, "SCRUBBED_RUN_ID")
}

/// Returns the scrubber that replaces the value of each named JSON field
/// with the string `"SCRUBBED"`: a string, a number, `true`, `false` or
/// `null`.
///
/// It matches the text of a field rather than parsing the document, so it
/// works on output that is nearly JSON, and it leaves an object or an array
/// as it is. A field name inside a string value is replaced too.
#[must_use]
#[expect(
    clippy::missing_panics_doc,
    reason = "the field names are escaped, so the pattern always compiles"
)]
pub fn scrub_json_fields(fields: &[&str]) -> Scrubber {
    if fields.is_empty() {
        return Scrubber::new(str::to_owned);
    }
    let names: Vec<String> = fields.iter().map(|field| regex::escape(field)).collect();
    let scalar = r#""(?:[^"\\]|\\.)*"|-?(?:0|[1-9][0-9]*)(?:\.[0-9]+)?(?:[eE][+-]?[0-9]+)?|(?:true|false|null)\b"#;
    let pattern = Regex::new(&format!(
        r#"("(?:{})"\s*:\s*)(?:{scalar})"#,
        names.join("|")
    ))
    .expect("a pattern of escaped names compiles");
    Scrubber::new(move |text| pattern.replace_all(text, r#"${1}"SCRUBBED""#).into_owned())
}

/// Returns `true` when `DOKIMI_ASSERT_UPDATE_GOLDEN` states a value other
/// than the empty string and `0`.
#[must_use]
pub fn should_update() -> bool {
    std::env::var_os(UPDATE_ENV).is_some_and(|set| !set.is_empty() && set != "0")
}

fn scrubbed(text: &str, scrubbers: &[Scrubber]) -> String {
    scrubbers
        .iter()
        .fold(text.to_owned(), |text, scrubber| scrubber.apply(&text))
}

/// One golden comparison: its seat, its assertion, its contract and its file.
struct Call<'s> {
    run: Run<'s>,
    assertion: &'static str,
    contract: String,
    path: PathBuf,
}

impl Call<'_> {
    #[track_caller]
    fn fault(&self, what: &str, error: impl fmt::Display) {
        self.run.fault(
            Mode::Fatal,
            self.assertion,
            &self.contract,
            format_args!("golden: {}: {what}: {error}", self.path.display()),
        );
    }

    /// Returns the golden file's text, `Ok(None)` for a missing file, and
    /// `Err` after a fault.
    ///
    /// Windows reports the read of a directory path that ends in a separator
    /// as not found. The file counts as missing only when no entry exists at
    /// the path.
    #[track_caller]
    fn read(&self) -> Result<Option<String>, ()> {
        match std::fs::read_to_string(&self.path) {
            Ok(text) => Ok(Some(text)),
            Err(error) if error.kind() == ErrorKind::NotFound && !self.path.exists() => Ok(None),
            Err(error) => {
                self.fault("the golden file cannot be read", error);
                Err(())
            }
        }
    }

    /// Writes `content` as the golden file, in a directory that it creates
    /// when missing, and passes.
    #[track_caller]
    fn write(&self, content: &str) {
        let written = self
            .path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(&self.path, content));
        match written {
            Ok(()) => self.run.pass(Mode::Fatal, self.assertion, &self.contract),
            Err(error) => self.fault("the golden file cannot be written", error),
        }
    }
}

#[track_caller]
fn file_call<'s>(seat: &'s dyn Seat, assertion: &'static str, path: PathBuf) -> Call<'s> {
    Call {
        run: Run::of(seat),
        assertion,
        contract: format!(
            "the golden file {} matches the output, and {UPDATE_ENV}=1 writes it",
            path.display()
        ),
        path,
    }
}

#[track_caller]
fn match_file(call: &Call<'_>, got: &str, update: bool, scrubbers: &[Scrubber]) {
    let mine = scrubbed(got, scrubbers);
    let Ok(recorded) = call.read() else { return };
    let theirs = recorded.map(|text| scrubbed(&text, scrubbers));
    if theirs.as_deref() == Some(mine.as_str()) {
        call.run.pass(Mode::Fatal, call.assertion, &call.contract);
        return;
    }
    if update {
        call.write(&mine);
        return;
    }
    call.run.fail(
        Mode::Fatal,
        call.assertion,
        &call.contract,
        vec![("want", Value::from(theirs)), ("got", Value::from(mine))],
    );
}

/// Ends the test when `got` differs from the golden file at `path`.
///
/// With `update`, a difference rewrites the golden file with the scrubbed
/// output, and the call passes. The scrubbers apply to both sides, in order.
#[track_caller]
pub fn matches_at(
    seat: &dyn Seat,
    path: impl AsRef<Path>,
    got: impl AsRef<str>,
    update: bool,
    scrubbers: &[Scrubber],
) {
    seat.helper();
    let call = file_call(seat, "golden-match-at", path.as_ref().to_path_buf());
    match_file(&call, got.as_ref(), update, scrubbers);
}

/// Ends the test when `got` differs from the golden file `name`, which
/// resolves against [`GOLDEN_DIR`].
///
/// With `update`, a difference rewrites the golden file with the scrubbed
/// output, and the call passes.
#[track_caller]
pub fn matches(
    seat: &dyn Seat,
    name: &str,
    got: impl AsRef<str>,
    update: bool,
    scrubbers: &[Scrubber],
) {
    seat.helper();
    let call = file_call(seat, "golden-match", Path::new(GOLDEN_DIR).join(name));
    match_file(&call, got.as_ref(), update, scrubbers);
}

/// Ends the test when the tree in the directory `got` differs from the
/// golden tree `name`, a directory that resolves against [`GOLDEN_DIR`].
///
/// ```no_run
/// use dokimi_assert::{Collector, golden};
///
/// let t = Collector::new();
/// let out = std::env::temp_dir().join("generated-api");
/// golden::matches_tree(&t, "api", &out, golden::should_update(), &[]);
/// ```
///
/// The name follows the rules of a path of a tree, so it cannot leave the
/// conventional directory. The comparison follows no link. It reads no mode
/// of the golden tree, and compares the owner's execute bit of each file,
/// the one bit that git records. The scrubbers apply to the content of each
/// file that is UTF-8 text, on both sides, in order. The record states
/// `want` and `got`, the trees of the entries at the first 64 paths that
/// differ, and `differences`, the number of those paths. A missing golden
/// directory fails with `want` null, `got` the first 64 entries of the
/// output, and `differences` the number of its entries.
///
/// With `update`, the call makes the golden directory equal the output and
/// passes. It removes each entry that the output lacks, writes each entry
/// that the directory lacks or has in another form, and writes the scrubbed
/// content. It removes a link, and never the entry that the link points to.
/// It writes a file that changes into a temporary file beside it and renames
/// the temporary file over it, so a write that fails leaves the file as it
/// was.
///
/// A name that is no path, an entry of either tree that is no file,
/// directory or link, and a tree or a golden directory that cannot be read
/// or written end the call with a fault.
#[track_caller]
pub fn matches_tree(
    seat: &dyn Seat,
    name: &str,
    got: impl AsRef<Path>,
    update: bool,
    scrubbers: &[Scrubber],
) {
    seat.helper();
    let dir = Path::new(GOLDEN_DIR).join(name);
    let contract = format!(
        "the golden tree {} matches the output, and {UPDATE_ENV}=1 writes it",
        dir.display()
    );
    let run = Run::of(seat);
    match compare_tree(&dir, name, got.as_ref(), update, scrubbers) {
        Err(fault) => run.fault(
            Mode::Fatal,
            "golden-match-tree",
            &contract,
            format_args!("golden::matches_tree: {fault}"),
        ),
        Ok(Some(record)) => run.fail(Mode::Fatal, "golden-match-tree", &contract, record.detail()),
        Ok(None) => run.pass(Mode::Fatal, "golden-match-tree", &contract),
    }
}

/// Compares the tree in `got` with the golden tree in `dir`, and updates the
/// golden tree with `update`. Returns the record of a comparison that fails,
/// and `None` for one that passes.
fn compare_tree(
    dir: &Path,
    name: &str,
    got: &Path,
    update: bool,
    scrubbers: &[Scrubber],
) -> Result<Option<Record>, Fault> {
    check_path(name).map_err(Fault::new)?;
    let output = read_tree(got, true)?.scrubbed(|text| scrubbed(text, scrubbers));
    let golden = match read_tree(dir, false) {
        Err(fault) if fault.finds_nothing() => None,
        golden => Some(golden?.scrubbed(|text| scrubbed(text, scrubbers))),
    };
    let paths = golden
        .as_ref()
        .map(|golden| differing(golden, &output, false));
    if update {
        if paths.as_ref().is_none_or(|paths| !paths.is_empty()) {
            update_tree(dir, &output)?;
        }
        return Ok(None);
    }
    Ok(match (golden, paths) {
        (Some(golden), Some(paths)) => {
            (!paths.is_empty()).then(|| Record::of(&golden, &output, &paths))
        }
        _ => Some(Record::missing(&output)),
    })
}

/// Ends the test when `got`, a JSON text, differs from the field `field` of
/// the golden JSON object at `path`.
///
/// Both sides are written again with two spaces of indent first, so a
/// difference of layout passes. A missing file or field fails unless
/// `update` is set, and an update writes the field and keeps every other
/// one. A `got` that is no JSON, and a golden file that is no JSON object,
/// end the call with a fault.
#[track_caller]
pub fn matches_json_field(
    seat: &dyn Seat,
    path: impl AsRef<Path>,
    field: &str,
    got: impl AsRef<str>,
    update: bool,
    scrubbers: &[Scrubber],
) {
    seat.helper();
    let path = path.as_ref().to_path_buf();
    let call = Call {
        run: Run::of(seat),
        assertion: "golden-match-json-field",
        contract: format!(
            "the field {:?} of the golden file {} matches the value, and {UPDATE_ENV}=1 writes it",
            field,
            path.display()
        ),
        path,
    };
    let value = match json::parse(got.as_ref(), JSON_DEPTH) {
        Ok(value) => value,
        Err(malformed) => {
            return call.fault(
                &format!("the value of the field {field:?} is no JSON"),
                malformed,
            );
        }
    };
    let mine = scrubbed(&value.pretty(), scrubbers);
    let Ok(recorded) = call.read() else { return };
    let mut members = match recorded.map(|text| json::parse(&text, JSON_DEPTH)) {
        None => Vec::new(),
        Some(Ok(Json::Object(members))) => members,
        Some(Ok(_)) => {
            return call.fault(
                "the golden file cannot be read as a JSON object",
                "it is no object",
            );
        }
        Some(Err(malformed)) => {
            return call.fault("the golden file cannot be read as a JSON object", malformed);
        }
    };
    let at = members.iter().position(|(name, _)| name == field);
    let theirs = at.map(|at| scrubbed(&members[at].1.pretty(), scrubbers));
    if theirs.as_deref() == Some(mine.as_str()) {
        call.run.pass(Mode::Fatal, call.assertion, &call.contract);
        return;
    }
    if update {
        match at {
            Some(at) => members[at].1 = value,
            None => members.push((field.to_owned(), value)),
        }
        call.write(&format!("{}\n", Json::Object(members).pretty()));
        return;
    }
    call.run.fail(
        Mode::Fatal,
        call.assertion,
        &call.contract,
        vec![
            ("want", Value::from(theirs)),
            ("got", Value::from(mine)),
            ("field", Value::from(field)),
        ],
    );
}
