//! The allocation ceilings in a test binary that installs no counting
//! allocator. Each ceiling ends its call with a fault that states the
//! allocator to install.

use dokimi_assert::bench::Contract;
use dokimi_assert::{Recorder, allocs_checked, check, prop, soft};

/// The text of the fault of every allocation ceiling without the counting
/// allocator.
const UNCOUNTED: &str = "no allocator counts allocations in this process: install \
                         dokimi_assert::bench::CountingAllocator as the global allocator";

/// Returns the verdict of the first call record of `seat`.
fn verdict(seat: &Recorder) -> String {
    let record: serde_json::Value =
        serde_json::from_str(&seat.records()[0]).expect("a call record is JSON");
    record["verdict"].as_str().unwrap_or_default().to_owned()
}

mod allocs_checked {
    use super::*;

    #[test]
    fn returns_the_reason_without_the_counting_allocator() {
        assert_eq!(allocs_checked(), Err(UNCOUNTED));
    }
}

mod max_allocs {
    use super::*;

    #[test]
    fn ends_the_call_with_a_fault() {
        let seat = Recorder::new();
        check::max_allocs(&seat, || {}, 0, "c");
        assert_eq!(seat.message(), UNCOUNTED);
        assert_eq!(verdict(&seat), "error");
    }

    #[test]
    fn ends_the_call_of_the_recording_surface_with_a_fault() {
        let seat = Recorder::new();
        soft::max_allocs(&seat, || {}, 0, "c");
        assert_eq!(seat.message(), UNCOUNTED);
        assert_eq!(seat.messages(), [] as [String; 0]);
    }
}

mod max_allocs_with_setup {
    use super::*;

    #[test]
    fn ends_the_call_with_a_fault() {
        let seat = Recorder::new();
        check::max_allocs_with_setup(&seat, || (), |()| {}, 0, "c");
        assert_eq!(seat.message(), UNCOUNTED);
        assert_eq!(verdict(&seat), "error");
    }
}

mod prop_max_allocs {
    use super::*;

    #[test]
    fn fails_each_case_with_the_fault_at_the_call_site_of_the_form() {
        let seat = Recorder::new();
        let line = line!() + 1;
        prop::max_allocs(
            &seat,
            |_: &u8| {},
            0,
            "c",
            prop::hermetic().seed(1).store(""),
        );
        let case = seat.failures()[0]
            .case_failure()
            .cloned()
            .expect("the run found a counterexample");
        assert_eq!(case.contract(), UNCOUNTED);
        assert_eq!(case.location().map(|at| at.line()), Some(line));
    }
}

mod contract {
    use super::*;

    #[test]
    fn ends_an_allocation_ceiling_with_a_fault() {
        let seat = Recorder::new();
        Contract::new(&seat, "c")
            .max_allocs(0)
            .run(1, || {})
            .check();
        assert_eq!(seat.message(), UNCOUNTED);
        assert_eq!(verdict(&seat), "error");
    }

    #[test]
    fn ends_a_byte_ceiling_with_a_fault() {
        let seat = Recorder::new();
        Contract::new(&seat, "c").max_bytes(0).run(1, || {}).check();
        assert_eq!(seat.message(), UNCOUNTED);
    }
}
