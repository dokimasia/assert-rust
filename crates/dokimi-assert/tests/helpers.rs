// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The helpers that more than one spec of the binary uses.

use dokimi_assert::files::{self, Tree, Workspace};
use dokimi_assert::history::{Call, History, Operation, Spec};
use dokimi_assert::{Collector, Failure, Recorder, Seat, Value};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

/// The spec of a register of integers: a write stores its argument, and a
/// read returns the stored value, 0 before the first write.
pub(crate) struct Register;

impl Spec for Register {
    type State = i64;

    fn initial(&self) -> i64 {
        0
    }

    fn next(&self, state: &i64, op: &Operation) -> Vec<i64> {
        match (op.name(), op.args()) {
            ("write", [Value::Int(value)]) => vec![i64::try_from(*value).unwrap_or_default()],
            ("read", []) if op.returned(*state) => vec![*state],
            _ => Vec::new(),
        }
    }
}

/// Records a write of `value` to the key `x` by `client`, which returns null.
pub(crate) fn write(history: &History, client: usize, value: i64) {
    history
        .invoke(client, "write", [value.into()], ["x".into()])
        .ok(Value::Null);
}

/// Records a read of the key `x` by `client`, which returns `value`.
pub(crate) fn read(history: &History, client: usize, value: i64) {
    history.invoke(client, "read", [], ["x".into()]).ok(value);
}

/// One micro-operation of a transaction: an append of a value to a key, a
/// read of a key with the list that it returned, a write of a value to a
/// register, or a read of a register with its value, which is `None` for the
/// initial value.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Op {
    Append(&'static str, i64),
    Read(&'static str, &'static [i64]),
    Write(&'static str, i64),
    Get(&'static str, Option<i64>),
}

impl Op {
    /// Returns the micro-operation as an argument of its invocation, with
    /// null in place of what a read returned.
    fn invoked(self) -> Value {
        match self {
            Self::Append(key, value) => {
                Value::List(vec!["append".into(), key.into(), value.into()])
            }
            Self::Write(key, value) => Value::List(vec!["write".into(), key.into(), value.into()]),
            Self::Read(key, _) | Self::Get(key, _) => {
                Value::List(vec!["read".into(), key.into(), Value::Null])
            }
        }
    }

    /// Returns the micro-operation as an ok output repeats it, with what a
    /// read returned.
    fn returned(self) -> Value {
        match self {
            Self::Append(..) | Self::Write(..) => self.invoked(),
            Self::Read(key, list) => Value::List(vec![
                "read".into(),
                key.into(),
                Value::List(list.iter().map(|value| Value::from(*value)).collect()),
            ]),
            Self::Get(key, value) => Value::List(vec![
                "read".into(),
                key.into(),
                value.map_or(Value::Null, Value::from),
            ]),
        }
    }
}

/// Records the invocation of a transaction of `ops` by `client`, and returns
/// its call.
pub(crate) fn txn<'h>(history: &'h History, client: usize, ops: &[Op]) -> Call<'h> {
    history.invoke(client, "txn", ops.iter().map(|op| op.invoked()), [])
}

/// Records a transaction of `ops` by `client` that commits.
pub(crate) fn commit(history: &History, client: usize, ops: &[Op]) {
    txn(history, client, ops).ok(Value::List(ops.iter().map(|op| op.returned()).collect()));
}

/// Returns the version of the definition that the conformance crate of the
/// workspace vendors, which its file `VERSION` contains. Every call record
/// states it.
pub(crate) fn definition() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../dokimi-assert-conformance/VERSION");
    std::fs::read_to_string(path)
        .expect("the conformance crate has the file VERSION")
        .trim()
        .to_owned()
}

/// A seat that keeps the reason of each skip, as the seat of a runner that
/// can skip a running test receives it. A failure panics, as it does on the
/// seat `Standard`.
#[derive(Default)]
pub(crate) struct Skipping(Mutex<Vec<String>>);

impl Skipping {
    /// Returns the reason of each skip, in the order of the calls.
    pub(crate) fn skips(&self) -> Vec<String> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl Seat for Skipping {
    fn fail(&self, message: &str) {
        panic!("{message}");
    }

    fn record(&self, message: &str) {
        panic!("{message}");
    }

    fn skip(&self, reason: &str) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(reason.to_owned());
    }
}

/// The names of the steps of a test, in the order in which they ran. A clone
/// appends to the same list.
#[derive(Clone, Default)]
pub(crate) struct Trace(Arc<Mutex<Vec<&'static str>>>);

impl Trace {
    /// Appends `name`.
    pub(crate) fn push(&self, name: &'static str) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(name);
    }

    /// Returns a cleanup that appends `name`.
    pub(crate) fn cleanup(&self, name: &'static str) -> Box<dyn FnOnce() + Send> {
        let trace = self.clone();
        Box::new(move || trace.push(name))
    }

    /// Returns the names in the order in which they ran.
    pub(crate) fn ran(&self) -> Vec<&'static str> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

/// Returns the call records of `seat` as JSON values, in the order of their
/// numbers.
pub(crate) fn records(seat: &Recorder) -> Vec<serde_json::Value> {
    seat.records()
        .iter()
        .map(|line| serde_json::from_str(line).expect("a call record is JSON"))
        .collect()
}

/// Runs `body` on a new recorder, and returns the one failure that it
/// reported.
///
/// # Panics
///
/// Panics when the body reported no failure, or more than one.
pub(crate) fn failure_of(body: impl FnOnce(&Recorder)) -> Failure {
    let seat = Recorder::new();
    body(&seat);
    let mut failures = seat.failures();
    assert_eq!(
        failures.len(),
        1,
        "the body reports one failure: {failures:?}"
    );
    failures.remove(0)
}

/// Runs `body` on a new recorder, and asserts that it reported no failure.
///
/// # Panics
///
/// Panics when the body reported a failure.
pub(crate) fn passes(body: impl FnOnce(&Recorder)) {
    let seat = Recorder::new();
    body(&seat);
    assert!(
        !seat.failed(),
        "the body passes, and it reported {:?}",
        seat.message()
    );
}

/// Returns the opaque value that the record of a value of a generic type
/// states: the value's `Debug` text.
pub(crate) fn opaque(text: &str) -> Value {
    Value::Opaque(text.to_owned())
}

/// Returns the message of the panic of `body`, or `None` when it returns.
pub(crate) fn panic_of(body: impl FnOnce()) -> Option<String> {
    catch_unwind(AssertUnwindSafe(body)).err().map(|raised| {
        raised
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| raised.downcast_ref::<&str>().map(|text| (*text).to_owned()))
            .unwrap_or_default()
    })
}

/// A directory of its own under the system's temporary directory, which the
/// drop removes with everything in it.
pub(crate) struct Scratch(PathBuf);

impl Scratch {
    /// Returns a new, empty directory.
    pub(crate) fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "dokimi-assert-it-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir_all(&path).expect("the temporary directory accepts a directory");
        Self(path)
    }

    /// Returns the path of `relative` inside the directory.
    pub(crate) fn join(&self, relative: impl AsRef<Path>) -> PathBuf {
        self.0.join(relative)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Returns a workspace of `tree`, on a seat that panics at a fault.
pub(crate) fn written(tree: &Tree) -> Workspace {
    files::workspace(&Collector::new(), tree)
}

/// Runs the ignored test `test` of this binary in a child process whose
/// working directory is `dir`, with the variables `vars`, and returns its
/// output. The child prints what its test prints.
pub(crate) fn in_child(test: &str, dir: &Path, vars: &[(&str, &str)]) -> Output {
    let binary = std::env::current_exe().expect("the test binary has a path");
    Command::new(binary)
        .args([
            test,
            "--exact",
            "--ignored",
            "--nocapture",
            "--test-threads",
            "1",
        ])
        .current_dir(dir)
        .envs(vars.iter().copied())
        .output()
        .expect("the test binary runs")
}

/// Returns each entry below `dir` as a line, in path order, read with the
/// standard library alone and without following a link: a directory as its
/// path and `/`, a file as its path and its content, and a link as its path
/// and its target. On Unix a directory and a file add their mode.
pub(crate) fn listing(dir: &Path) -> Vec<String> {
    let mut lines = Vec::new();
    list(dir, "", &mut lines);
    lines.sort();
    lines
}

fn list(dir: &Path, under: &str, lines: &mut Vec<String>) {
    for item in std::fs::read_dir(dir.join(under)).expect("the directory reads") {
        let name = item.expect("the directory lists its entries").file_name();
        let name = name.to_str().expect("the name is UTF-8 text");
        let path = if under.is_empty() {
            name.to_owned()
        } else {
            format!("{under}/{name}")
        };
        let at = dir.join(&path);
        let meta = std::fs::symlink_metadata(&at).expect("the entry reads");
        if meta.is_symlink() {
            let target = std::fs::read_link(&at).expect("the link reads");
            lines.push(format!("{path} -> {}", target.display()));
        } else if meta.is_dir() {
            lines.push(format!("{path}/{}", mode(&meta)));
            list(dir, &path, lines);
        } else {
            let content = std::fs::read(&at).expect("the file reads");
            lines.push(format!(
                "{path} {:?}{}",
                String::from_utf8_lossy(&content),
                mode(&meta)
            ));
        }
    }
}

#[cfg(unix)]
fn mode(meta: &std::fs::Metadata) -> String {
    use std::os::unix::fs::PermissionsExt;
    format!(" {:#o}", meta.permissions().mode() & 0o777)
}

#[cfg(not(unix))]
fn mode(_: &std::fs::Metadata) -> String {
    String::new()
}

/// One assertion of a surface, called with arguments that fail it.
pub(crate) struct Case {
    /// The name of the function.
    pub(crate) name: &'static str,
    /// Calls the function with arguments that fail it.
    pub(crate) give: fn(&Recorder),
    /// The id of the assertion that the failure names.
    pub(crate) want_assertion: &'static str,
}

/// Returns a call of each assertion of the surface `$surface` that fails, as a
/// [`Case`]. Both surfaces state the same assertions, so one list states the
/// cases of both.
macro_rules! failing_calls {
    ($surface:ident) => {{
        use dokimi_assert::{Cancel, Stop};
        use std::cell::Cell;
        use std::hint::black_box;
        use std::time::Duration;
        use $crate::helpers::Case;
        vec![
            Case {
                name: "equal",
                give: |t| $surface::equal(t, 1, 2, "c"),
                want_assertion: "equal",
            },
            Case {
                name: "not_equal",
                give: |t| $surface::not_equal(t, 1, 1, "c"),
                want_assertion: "not-equal",
            },
            Case {
                name: "is_true",
                give: |t| $surface::is_true(t, false, "c"),
                want_assertion: "true",
            },
            Case {
                name: "is_false",
                give: |t| $surface::is_false(t, true, "c"),
                want_assertion: "false",
            },
            Case {
                name: "is_none",
                give: |t| $surface::is_none(t, Some(1), "c"),
                want_assertion: "nil",
            },
            Case {
                name: "is_some",
                give: |t| $surface::is_some(t, None::<i32>, "c"),
                want_assertion: "not-nil",
            },
            Case {
                name: "length",
                give: |t| $surface::length(t, "ab", 3, "c"),
                want_assertion: "length",
            },
            Case {
                name: "is_empty",
                give: |t| $surface::is_empty(t, "ab", "c"),
                want_assertion: "empty",
            },
            Case {
                name: "is_not_empty",
                give: |t| $surface::is_not_empty(t, "", "c"),
                want_assertion: "not-empty",
            },
            Case {
                name: "contains",
                give: |t| $surface::contains(t, "ab", "z", "c"),
                want_assertion: "contains",
            },
            Case {
                name: "not_contains",
                give: |t| $surface::not_contains(t, "ab", "a", "c"),
                want_assertion: "not-contains",
            },
            Case {
                name: "contains_in_order",
                give: |t| $surface::contains_in_order(t, "ba", &["a", "b"], "c"),
                want_assertion: "contains-in-order",
            },
            Case {
                name: "is_permutation",
                give: |t| $surface::is_permutation(t, &[1], &[2], "c"),
                want_assertion: "permutation",
            },
            Case {
                name: "has_prefix",
                give: |t| $surface::has_prefix(t, "ab", "b", "c"),
                want_assertion: "has-prefix",
            },
            Case {
                name: "has_suffix",
                give: |t| $surface::has_suffix(t, "ab", "a", "c"),
                want_assertion: "has-suffix",
            },
            Case {
                name: "matches",
                give: |t| $surface::matches(t, "ab", "^b", "c"),
                want_assertion: "matches",
            },
            Case {
                name: "close_to",
                give: |t| $surface::close_to(t, 2.0, 1.0, 0.5, "c"),
                want_assertion: "close-to",
            },
            Case {
                name: "in_range",
                give: |t| $surface::in_range(t, 5, 0.0, 1.0, "c"),
                want_assertion: "in-range",
            },
            Case {
                name: "pairwise",
                give: |t| $surface::pairwise(t, &[2, 1], |a, b| a < b, "c"),
                want_assertion: "pairwise",
            },
            Case {
                name: "no_error",
                give: |t| $surface::no_error(t, &Err::<(), _>(Stop::Cancelled), "c"),
                want_assertion: "err-absent",
            },
            Case {
                name: "has_error",
                give: |t| $surface::has_error(t, &Ok::<_, Stop>(1), "c"),
                want_assertion: "err-present",
            },
            Case {
                name: "error_is",
                give: |t| $surface::error_is(t, &Stop::Cancelled, &Stop::DeadlineExceeded, "c"),
                want_assertion: "err-is",
            },
            Case {
                name: "error_is_not",
                give: |t| $surface::error_is_not(t, &Stop::Cancelled, &Stop::Cancelled, "c"),
                want_assertion: "err-is-not",
            },
            Case {
                name: "error_as",
                give: |t| {
                    let _ = $surface::error_as::<std::fmt::Error>(t, &Stop::Cancelled, "c");
                },
                want_assertion: "err-as",
            },
            Case {
                name: "panics",
                give: |t| drop($surface::panics(t, || {}, "c")),
                want_assertion: "throws",
            },
            Case {
                name: "does_not_panic",
                give: |t| $surface::does_not_panic(t, || panic!("the body panics"), "c"),
                want_assertion: "not-throws",
            },
            Case {
                name: "honours_cancellation",
                give: |t| $surface::honours_cancellation(t, |_| Ok::<(), Stop>(()), "c"),
                want_assertion: "honours-cancellation",
            },
            Case {
                name: "honours_deadline",
                give: |t| $surface::honours_deadline(t, |_| Ok::<(), Stop>(()), "c"),
                want_assertion: "honours-deadline",
            },
            Case {
                name: "completes_within",
                give: |t| $surface::completes_within(t, Duration::ZERO, |_| Ok::<(), ()>(()), "c"),
                want_assertion: "completes-within",
            },
            Case {
                name: "is_pure",
                give: |t| {
                    let state = Cell::new(0);
                    $surface::is_pure(t, || state.get(), || state.set(1), "c");
                },
                want_assertion: "pure",
            },
            Case {
                name: "is_not_pure",
                give: |t| $surface::is_not_pure(t, || 1, || {}, "c"),
                want_assertion: "not-pure",
            },
            Case {
                name: "none_handle_safe",
                give: |t| {
                    $surface::none_handle_safe(
                        t,
                        |cancel: Option<&Cancel>| {
                            let _ = cancel.expect("the subject expects a handle");
                            Ok::<(), ()>(())
                        },
                        "c",
                    );
                },
                want_assertion: "nil-context-safe",
            },
            Case {
                name: "is_idempotent",
                give: |t| {
                    let state = Cell::new(0);
                    let add = |n: &i32| {
                        state.set(state.get() + n);
                        Ok::<(), ()>(())
                    };
                    $surface::is_idempotent(t, add, 1, || state.get(), "c");
                },
                want_assertion: "idempotent",
            },
            Case {
                name: "accumulates",
                give: |t| $surface::accumulates(t, |_: &i32| Ok::<(), ()>(()), 1, || 0, "c"),
                want_assertion: "accumulates",
            },
            Case {
                name: "is_deterministic",
                give: |t| {
                    let calls = Cell::new(0);
                    let count = |_: &i32| {
                        calls.set(calls.get() + 1);
                        Ok::<i32, ()>(calls.get())
                    };
                    $surface::is_deterministic(t, count, 0, "c");
                },
                want_assertion: "deterministic",
            },
            Case {
                name: "is_commutative",
                give: |t| $surface::is_commutative(t, |a: &i32, b: &i32| a - b, 1, 2, "c"),
                want_assertion: "commutative",
            },
            Case {
                name: "is_associative",
                give: |t| $surface::is_associative(t, |a: &i32, b: &i32| a - b, 1, 2, 3, "c"),
                want_assertion: "associative",
            },
            Case {
                name: "round_trip",
                give: |t| {
                    $surface::round_trip(
                        t,
                        |n: &i32| Ok::<i32, ()>(n + 1),
                        |n: &i32| Ok::<i32, ()>(*n),
                        1,
                        "c",
                    )
                },
                want_assertion: "round-trip",
            },
            Case {
                name: "has_stable_order",
                give: |t| {
                    let calls = Cell::new(0);
                    let iterate = || {
                        calls.set(calls.get() + 1);
                        Ok::<_, ()>(vec![calls.get()])
                    };
                    $surface::has_stable_order(t, iterate, "c");
                },
                want_assertion: "stable-order",
            },
            Case {
                name: "no_duplicates",
                give: |t| $surface::no_duplicates(t, || Ok::<_, ()>(vec![1, 1]), "c"),
                want_assertion: "no-duplicates",
            },
            Case {
                name: "is_monotonic",
                give: |t| {
                    let reading = Cell::new(1);
                    let fall = || {
                        reading.set(0);
                        Ok::<(), ()>(())
                    };
                    $surface::is_monotonic(t, || reading.get(), fall, 1, "c");
                },
                want_assertion: "monotonic",
            },
            Case {
                name: "is_total",
                give: |t| $surface::is_total(t, |_: &i32| Err::<(), _>(()), &[1], "c"),
                want_assertion: "total",
            },
            Case {
                name: "fails_after_close",
                give: |t| {
                    $surface::fails_after_close(
                        t,
                        || Ok::<(), ()>(()),
                        || Ok::<(), Stop>(()),
                        &Stop::Cancelled,
                        "c",
                    );
                },
                want_assertion: "after-close",
            },
            Case {
                name: "is_poisoned",
                give: |t| $surface::is_poisoned(t, || {}, || Ok::<(), ()>(()), "c"),
                want_assertion: "poisoned",
            },
            Case {
                name: "eventually",
                give: |t| {
                    let attempt = |s: &dyn dokimi_assert::Seat| {
                        dokimi_assert::check::is_true(s, false, "settled")
                    };
                    $surface::eventually(t, Duration::ZERO, Duration::ZERO, attempt, "c");
                },
                want_assertion: "eventually",
            },
            Case {
                name: "eventually_true",
                give: |t| $surface::eventually_true(t, Duration::ZERO, || false, "c"),
                want_assertion: "eventually-true",
            },
            Case {
                name: "max_allocs",
                give: |t| {
                    $surface::max_allocs(
                        t,
                        || {
                            black_box(vec![1_u8]);
                        },
                        0,
                        "c",
                    );
                },
                want_assertion: "max-allocs",
            },
            Case {
                name: "max_allocs_with_setup",
                give: |t| {
                    $surface::max_allocs_with_setup(
                        t,
                        || (),
                        |()| {
                            black_box(vec![1_u8]);
                        },
                        0,
                        "c",
                    );
                },
                want_assertion: "max-allocs-with-setup",
            },
        ]
    }};
}

pub(crate) use failing_calls;
