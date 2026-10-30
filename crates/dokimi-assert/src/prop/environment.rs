// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The variables that set the defaults of every property of a test run.
//!
//! A hermetic run reads none of them, and an unset or empty variable states
//! no value.
//!
//! - `DOKIMI_ASSERT_PROP_SEED` states the seed, in decimal, of every run
//!   without a seed of its own.
//! - `DOKIMI_ASSERT_PROP_PROFILE` names the profile: `default` draws a random
//!   seed for each run, `ci` derives the seed of each run from its contract,
//!   and `campaign` runs each property as a campaign.
//! - `DOKIMI_ASSERT_PROP_BUDGET` states the length of a campaign in whole
//!   seconds.
//! - `DOKIMI_ASSERT_PROP_REPLAY` states the token that every run without a
//!   replay of its own replays.
//!
//! A mutation run sets `DOKIMI_MUTATE_MUTANT` in each process that it
//! instruments. Such a process derives every seed from its contract, runs no
//! campaign and writes no store entry.

use super::engine::source::mix;
use crate::seat::Seat;
use crate::verdict;
use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};

/// The variables of the engine.
pub(crate) const SEED_VARIABLE: &str = "DOKIMI_ASSERT_PROP_SEED";
pub(crate) const PROFILE_VARIABLE: &str = "DOKIMI_ASSERT_PROP_PROFILE";
pub(crate) const BUDGET_VARIABLE: &str = "DOKIMI_ASSERT_PROP_BUDGET";
pub(crate) const REPLAY_VARIABLE: &str = "DOKIMI_ASSERT_PROP_REPLAY";

/// The variable that a mutation run sets in every process that it
/// instruments.
const MUTANT_VARIABLE: &str = "DOKIMI_MUTATE_MUTANT";

/// A budget is below 2^33 seconds in every language.
const BUDGET_LIMIT: u64 = 1 << 33;

/// A set of defaults for a whole test run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Profile {
    Default,
    Ci,
    Campaign,
}

/// A fault of a variable: the variable and what is wrong with its value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Refused {
    pub(crate) at: &'static str,
    pub(crate) reason: String,
}

/// Returns the value of a variable, or `None` for an unset or empty one.
fn variable(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

/// Returns the number that a decimal text states, below `limit`.
fn decimal(text: &str, limit: u128) -> Option<u128> {
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let significant = text.trim_start_matches('0');
    let number: u128 = if significant.is_empty() {
        0
    } else {
        significant.parse().ok()?
    };
    (number < limit).then_some(number)
}

/// Returns `true` when this process runs under a mutation run.
pub(crate) fn mutated() -> bool {
    std::env::var_os(MUTANT_VARIABLE).is_some()
}

/// Returns the profile of the environment.
pub(crate) fn profile() -> Result<Profile, Refused> {
    match variable(PROFILE_VARIABLE).as_deref() {
        None | Some("default") => Ok(Profile::Default),
        Some("ci") => Ok(Profile::Ci),
        Some("campaign") => Ok(Profile::Campaign),
        Some(other) => Err(Refused {
            at: PROFILE_VARIABLE,
            reason: format!("{other:?} is not one of the profiles default, ci and campaign"),
        }),
    }
}

/// Returns the seed of a run of `contract`.
///
/// The stated seed comes first. A run that is not hermetic then takes the
/// seed of `DOKIMI_ASSERT_PROP_SEED`. A process of a mutation run and a run
/// under the `ci` profile derive the seed from the contract, and every other
/// run draws a random seed. A run that is not hermetic checks the profile
/// first, so a misspelled profile fails every such run.
pub(crate) fn seed(contract: &str, given: Option<u64>, hermetic: bool) -> Result<u64, Refused> {
    let profile = if hermetic {
        Profile::Default
    } else {
        profile()?
    };
    if let Some(given) = given {
        return Ok(given);
    }
    if let Some(text) = (!hermetic).then(|| variable(SEED_VARIABLE)).flatten() {
        return decimal(&text, 1 << 64)
            .and_then(|seed| u64::try_from(seed).ok())
            .ok_or_else(|| Refused {
                at: SEED_VARIABLE,
                reason: format!("{text:?} is not a decimal number below 2^64"),
            });
    }
    if mutated() || profile == Profile::Ci {
        return Ok(mix(contract.as_bytes()));
    }
    Ok(random_seed())
}

/// Returns the seed that a run of `contract` takes without the option
/// [`seed`](super::seed).
///
/// The run is not hermetic. `run_seed` returns the value of
/// `DOKIMI_ASSERT_PROP_SEED` when the variable is set. In a process of a
/// mutation run, and under the `ci` profile, it returns the seed that a run
/// derives from the contract. Otherwise each call returns a new random seed.
/// A caller that passes the result to `seed` in each of its runs gives every
/// run the seed that its first run would take:
///
/// ```
/// use dokimi_assert::{Collector, check, prop};
///
/// let t = Collector::new();
/// let seed = prop::run_seed(&t, "the suite of the ledger");
/// prop::for_all(&t, "a digit is below 10", prop::seed(seed), |c| {
///     let n = c.draw(&prop::integer(0..=9_u8), "n");
///     check::is_true(c, n < 10, "the digit is below 10");
/// });
/// ```
///
/// A malformed `DOKIMI_ASSERT_PROP_SEED` and a profile other than `default`,
/// `ci` and `campaign` end the call with a fault of `prop::run_seed`, as
/// [`fault`](crate::fault) ends one. `run_seed` then returns 0.
#[track_caller]
pub fn run_seed(seat: &dyn Seat, contract: &str) -> u64 {
    seat.helper();
    seed(contract, None, false).unwrap_or_else(|refused| {
        verdict::end(
            seat,
            format_args!("prop::run_seed: {}: {}", refused.at, refused.reason),
        );
        0
    })
}

/// Returns a seed that differs from run to run: the random keys of the
/// standard library's hasher, which differ for each hasher of a process and
/// for each process.
fn random_seed() -> u64 {
    RandomState::new().build_hasher().finish()
}

/// Returns the length of a campaign in whole seconds, and 0 for a run that
/// is no campaign: a hermetic run, a run in a process of a mutation run, and
/// a run under another profile.
pub(crate) fn budget(hermetic: bool) -> Result<u64, Refused> {
    if hermetic || mutated() || profile()? != Profile::Campaign {
        return Ok(0);
    }
    let text = std::env::var(BUDGET_VARIABLE).unwrap_or_default();
    decimal(&text, u128::from(BUDGET_LIMIT))
        .and_then(|seconds| u64::try_from(seconds).ok())
        .filter(|seconds| *seconds >= 1)
        .ok_or_else(|| Refused {
            at: BUDGET_VARIABLE,
            reason: format!("{text:?} is not a whole number of seconds above 0"),
        })
}

/// Returns the token that a run replays and where it came from: the stated
/// token first, then `DOKIMI_ASSERT_PROP_REPLAY` for a run that is not
/// hermetic.
pub(crate) fn token(given: Option<&str>, hermetic: bool) -> Option<(String, &'static str)> {
    if let Some(given) = given {
        return Some((given.to_owned(), "replay"));
    }
    (!hermetic)
        .then(|| variable(REPLAY_VARIABLE))
        .flatten()
        .map(|token| (token, REPLAY_VARIABLE))
}
