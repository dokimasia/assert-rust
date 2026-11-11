// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `stateful::strategy`, the strategies of the task scheduler.

use crate::helpers::panic_of;
use dokimi_assert::stateful;

mod uniform {
    use super::*;

    #[test]
    fn differs_from_every_depth_of_pct() {
        assert_ne!(stateful::uniform(), stateful::pct(1));
        assert_eq!(stateful::uniform(), stateful::uniform());
    }

    #[test]
    fn releases_at_most_a_million_tasks_in_one_call() {
        assert_eq!(
            format!("{:?}", stateful::uniform()),
            "Strategy { depth: 0, max_releases: 1000000 }"
        );
    }
}

mod pct {
    use super::*;

    #[test]
    fn keeps_its_depth() {
        assert_eq!(stateful::pct(3), stateful::pct(3));
        assert_ne!(stateful::pct(3), stateful::pct(2));
    }

    #[test]
    fn panics_for_a_depth_of_0() {
        assert_eq!(
            panic_of(|| _ = stateful::pct(0)).as_deref(),
            Some("stateful: pct(0) is no depth of 1 or more")
        );
    }
}

mod max_releases {
    use super::*;

    #[test]
    fn starts_a_chain_from_the_uniform_strategy() {
        assert_eq!(
            format!("{:?}", stateful::max_releases(3)),
            "Strategy { depth: 0, max_releases: 3 }"
        );
    }

    #[test]
    fn keeps_the_depth_of_the_strategy() {
        assert_eq!(
            format!("{:?}", stateful::pct(2).max_releases(7)),
            "Strategy { depth: 2, max_releases: 7 }"
        );
    }

    #[test]
    fn panics_for_a_limit_of_0() {
        let message = Some("stateful: max_releases(0) is no count of 1 or more");
        assert_eq!(
            panic_of(|| _ = stateful::max_releases(0)).as_deref(),
            message
        );
        assert_eq!(
            panic_of(|| _ = stateful::pct(2).max_releases(0)).as_deref(),
            message
        );
    }
}
