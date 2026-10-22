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
