//! The record a failing assertion reports, and the sentence it renders to.

use dokimi_assert::check;
use dokimi_assert::failure::{Failure, Where};
use dokimi_assert::seat::Recorder;

/// A record carrying nothing, for the assertions that report the contract alone.
fn bare(assertion: &'static str, contract: &str) -> Failure {
    Failure {
        assertion,
        contract: contract.to_owned(),
        detail: Vec::new(),
        where_at: Where {
            file: "here.rs",
            line: 1,
        },
    }
}

#[test]
fn a_record_carrying_nothing_renders_the_contract_alone() {
    assert_eq!(bare("true", "the flag is set").render(), "the flag is set");
}

#[test]
fn the_detail_reads_in_the_order_the_assertion_states_it() {
    let mut held = bare("length", "every item comes back");
    held.detail = vec![
        ("want", "3".to_owned().into()),
        ("got", "2".to_owned().into()),
    ];

    assert_eq!(held.render(), "every item comes back: want 3, got 2");
}

#[test]
fn detail_answers_a_field_by_name_and_nothing_for_one_it_lacks() {
    let mut held = bare("equal", "the count is right");
    held.detail = vec![
        ("want", "2".to_owned().into()),
        ("got", "1".to_owned().into()),
    ];

    assert_eq!(
        held.detail("want").map(ToString::to_string).as_deref(),
        Some("2")
    );
    assert_eq!(held.detail("absent"), None);
}

#[test]
fn a_reported_failure_carries_the_call_site() {
    let seat = Recorder::new();
    check::equal(&seat, &1_i64, &2_i64, "the values match");

    let held = &seat.failures()[0];
    assert!(
        held.where_at.file.ends_with("failure.rs"),
        "the record names the file the caller wrote in, got {}",
        held.where_at.file
    );
    assert!(held.where_at.line > 0, "the record names a line");
}
