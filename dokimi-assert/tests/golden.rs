//! Comparison against a recorded file.

use dokimi_assert::golden;
use dokimi_assert::seat::Recorder;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// A directory of this test run's own, since the crate carries no temp-file crate.
struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let at = std::env::temp_dir().join(format!(
            "dokimi-golden-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir_all(&at).expect("a scratch directory is creatable");
        Self(at)
    }

    fn file(&self, name: &str, content: &str) -> PathBuf {
        let path = self.0.join(name);
        std::fs::write(&path, content).expect("the scratch file is writable");
        path
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn matching_content_passes_and_differing_content_is_reported() {
    let dir = Scratch::new();
    let path = dir.file("out.txt", "recorded output");

    let seat = Recorder::new();
    golden::matches_at(&seat, &path, "recorded output", &[]);
    assert!(!seat.failed(), "{}", seat.message());

    let failing = Recorder::new();
    golden::matches_at(&failing, &path, "something else", &[]);
    assert!(failing.failed(), "changed output must be reported");
    assert!(
        failing.message().contains("does not match"),
        "{}",
        failing.message()
    );
}

#[test]
fn a_missing_file_says_how_to_record_it() {
    let dir = Scratch::new();

    let seat = Recorder::new();
    golden::matches_at(&seat, &dir.path("absent.txt"), "content", &[]);

    assert!(seat.failed(), "a file that is not there cannot match");
    assert!(
        seat.message().contains(golden::UPDATE_ENV),
        "{}",
        seat.message()
    );
}

#[test]
fn scrubbers_replace_what_changes_every_run() {
    let dir = Scratch::new();
    let path = dir.file("scrubbed.txt", "at SCRUBBED_TIMESTAMP");

    let seat = Recorder::new();
    golden::matches_at(
        &seat,
        &path,
        "at 2026-08-30T11:22:33Z",
        &[golden::scrub_timestamps()],
    );

    assert!(!seat.failed(), "{}", seat.message());
}

#[test]
fn every_scrubber_replaces_its_own_shape() {
    let dir = Scratch::new();
    let path = dir.file(
        "all.txt",
        "run SCRUBBED_RUN_ID digest SCRUBBED_HASH at SCRUBBED_TIMESTAMP",
    );

    let seat = Recorder::new();
    golden::matches_at(
        &seat,
        &path,
        "run 3f2504e0-4f89-11d3-9a0c-0305e82c3301 \
         digest 9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08 \
         at 2026-08-30T11:22:33Z",
        &[
            golden::scrub_run_ids(),
            golden::scrub_hashes(),
            golden::scrub_timestamps(),
        ],
    );

    assert!(!seat.failed(), "{}", seat.message());
}

#[test]
fn a_nested_object_is_read_whole_not_cut_at_its_first_brace() {
    // A regular expression cannot find where a value ends. Scanning can.
    let dir = Scratch::new();
    let path = dir.file(
        "golden.json",
        "{\n  \"items\": {\"a\": 1},\n  \"other\": 2\n}",
    );

    let passing = Recorder::new();
    golden::matches_json_field(&passing, &path, "items", "{\"a\": 1}", &[]);
    assert!(!passing.failed(), "{}", passing.message());

    let failing = Recorder::new();
    golden::matches_json_field(&failing, &path, "items", "{\"a\": 2}", &[]);
    assert!(failing.failed(), "a changed nested value must be reported");
}

#[test]
fn an_array_of_objects_is_read_whole() {
    let dir = Scratch::new();
    let path = dir.file("golden.json", "{\n  \"items\": [{\"a\": 1}, {\"b\": 2}]\n}");

    let seat = Recorder::new();
    golden::matches_json_field(&seat, &path, "items", "[{\"a\": 1}, {\"b\": 2}]", &[]);
    assert!(!seat.failed(), "{}", seat.message());
}

#[test]
fn a_brace_inside_a_string_is_not_a_structural_brace() {
    let dir = Scratch::new();
    let path = dir.file("golden.json", "{\n  \"items\": \"a } and a , inside\"\n}");

    let seat = Recorder::new();
    golden::matches_json_field(&seat, &path, "items", "\"a } and a , inside\"", &[]);
    assert!(!seat.failed(), "{}", seat.message());
}

#[test]
fn a_missing_field_says_so() {
    let dir = Scratch::new();
    let path = dir.file("golden.json", "{\"other\": 1}");

    let seat = Recorder::new();
    golden::matches_json_field(&seat, &path, "absent", "[1]", &[]);
    assert!(seat.failed(), "a field that is not there cannot match");
    assert!(seat.message().contains("absent"), "{}", seat.message());
}
