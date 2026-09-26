//! An allocation ceiling in a test binary that installs no counting allocator.
//!
//! Each integration test is its own binary, so this one runs on the system allocator and
//! nothing counts.

use dokimi_assert::check;
use dokimi_assert::seat::Recorder;
use std::cell::Cell;

#[test]
fn without_the_allocator_it_reports_that_nothing_counted() {
    let calls = Cell::new(0_usize);
    let seat = Recorder::new();
    check::max_allocs(
        &seat,
        || calls.set(calls.get() + 1),
        0,
        "the body allocates nothing",
    );

    assert!(seat.failed(), "a ceiling nothing counted must not pass");
    assert!(
        seat.message().contains("nothing counted"),
        "{}",
        seat.message()
    );
    assert!(calls.get() > 0, "the body still runs");
}
