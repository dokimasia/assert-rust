//! The settings of a property: a builder whose methods each state one
//! setting.

use super::engine::case::MAX_CHOICES;
use super::engine::runner::{DEFAULT_CASES, Requirement};
use super::engine::shrink::DEFAULT_BUDGET;
use super::generator::Generator;
use std::fmt::{self, Debug};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// The time that shrinking and explaining a failure may take by default.
const DEFAULT_SHRINK_TIME: Duration = Duration::from_secs(30);

/// A wrapper of each run of a property's body: it calls the run once, inside
/// an environment that it chooses, and returns after the run returns.
pub(crate) type Wrapper = Arc<dyn Fn(&mut dyn FnMut()) + Send + Sync>;

/// The settings of a property over inputs of type `T`.
///
/// Each method states one setting and returns the settings, so a caller
/// chains them. A free function of the same name, such as [`cases`], starts
/// the chain, and `Default::default()` states no setting:
///
/// ```
/// use dokimi_assert::{Collector, check, prop};
///
/// let t = Collector::new();
/// prop::for_all(&t, "a byte string decodes", prop::cases(20).seed(7), |c| {
///     let data = c.draw(&prop::bytes(..=16), "data");
///     check::is_true(c, data.len() <= 16, "the data is short");
/// });
/// ```
///
/// `T` is the input of a property form, so [`Options::using`],
/// [`Options::example`] and [`Options::examples`] state values that the
/// compiler checks against the form. [`for_all`](crate::prop::for_all) takes
/// `Options<()>`.
#[must_use]
pub struct Options<T = ()> {
    pub(crate) cases: usize,
    pub(crate) seed: Option<u64>,
    pub(crate) replay: Option<String>,
    pub(crate) requirements: Vec<Requirement>,
    pub(crate) shrink: usize,
    pub(crate) shrink_time: Duration,
    pub(crate) max_choices: usize,
    pub(crate) store: Option<PathBuf>,
    pub(crate) explain: bool,
    pub(crate) workers: usize,
    pub(crate) hermetic: bool,
    pub(crate) around: Option<Wrapper>,
    pub(crate) draws: Option<String>,
    pub(crate) using: Option<Generator<T>>,
    pub(crate) examples: Vec<Vec<T>>,
}

impl<T: Clone> Clone for Options<T> {
    fn clone(&self) -> Self {
        Self {
            cases: self.cases,
            seed: self.seed,
            replay: self.replay.clone(),
            requirements: self.requirements.clone(),
            shrink: self.shrink,
            shrink_time: self.shrink_time,
            max_choices: self.max_choices,
            store: self.store.clone(),
            explain: self.explain,
            workers: self.workers,
            hermetic: self.hermetic,
            around: self.around.clone(),
            draws: self.draws.clone(),
            using: self.using.clone(),
            examples: self.examples.clone(),
        }
    }
}

impl<T> Debug for Options<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Options")
            .field("cases", &self.cases)
            .field("seed", &self.seed)
            .field("replay", &self.replay)
            .field("shrink", &self.shrink)
            .field("shrink_time", &self.shrink_time)
            .field("max_choices", &self.max_choices)
            .field("store", &self.store)
            .field("explain", &self.explain)
            .field("workers", &self.workers)
            .field("hermetic", &self.hermetic)
            .finish_non_exhaustive()
    }
}

/// States 100 cases, 2,000 shrink runs, 30 seconds of shrinking, 8,192
/// choices per case, the store of the test, an explanation of each failure
/// and one worker.
impl<T> Default for Options<T> {
    fn default() -> Self {
        Self {
            cases: DEFAULT_CASES,
            seed: None,
            replay: None,
            requirements: Vec::new(),
            shrink: DEFAULT_BUDGET,
            shrink_time: DEFAULT_SHRINK_TIME,
            max_choices: MAX_CHOICES,
            store: None,
            explain: true,
            workers: 1,
            hermetic: false,
            around: None,
            draws: None,
            using: None,
            examples: Vec::new(),
        }
    }
}

impl<T> Options<T> {
    /// States the valid cases that the run aims for.
    ///
    /// # Panics
    ///
    /// Panics for `n` of 0.
    pub fn cases(mut self, n: usize) -> Self {
        assert!(n >= 1, "prop: cases(0) tests nothing");
        self.cases = n;
        self
    }

    /// States the seed of the run, which takes precedence over
    /// `DOKIMI_ASSERT_PROP_SEED`.
    pub fn seed(mut self, seed: u64) -> Self {
        self.seed = Some(seed);
        self
    }

    /// States a replay token: the run tries the one case that the token
    /// records, reports a failure as found, and passes a pass.
    pub fn replay(mut self, token: &str) -> Self {
        self.replay = Some(token.to_owned());
        self
    }

    /// Requires `label` to count at least `share` of the valid cases. The run
    /// decides the requirement with the Wilson score interval at a certainty
    /// of 10^9, and runs up to eight times as many cases while it is
    /// undecided.
    ///
    /// # Panics
    ///
    /// Panics for a share outside `[0, 1]`.
    pub fn require(mut self, label: &str, share: f64) -> Self {
        assert!(
            (0.0..=1.0).contains(&share),
            "prop: require({label:?}, {share}) states no share"
        );
        self.requirements.push(Requirement {
            label: label.to_owned(),
            share,
        });
        self
    }

    /// States the runs that shrinking and explaining every failure may spend.
    /// A budget of 0 reports the first failing case as found.
    pub fn shrink(mut self, runs: usize) -> Self {
        self.shrink = runs;
        self
    }

    /// States the time that shrinking and explaining may take on the platform
    /// clock. A limit of zero leaves the time unlimited.
    pub fn shrink_time(mut self, limit: Duration) -> Self {
        self.shrink_time = limit;
        self
    }

    /// States the most choices that one case may make. A case past the cap
    /// is rejected.
    ///
    /// # Panics
    ///
    /// Panics for `n` of 0.
    pub fn max_choices(mut self, n: usize) -> Self {
        assert!(n >= 1, "prop: max_choices(0) admits no choice");
        self.max_choices = n;
        self
    }

    /// States the directory of the store, in place of the store of the test.
    /// An empty path turns the store off.
    pub fn store(mut self, dir: impl AsRef<Path>) -> Self {
        self.store = Some(dir.as_ref().to_path_buf());
        self
    }

    /// States whether the run explains each draw of a counterexample.
    pub fn explain(mut self, enabled: bool) -> Self {
        self.explain = enabled;
        self
    }

    /// States the cases that run at once on threads. The run reports what a
    /// run on one worker reports.
    ///
    /// # Panics
    ///
    /// Panics for `n` of 0.
    pub fn workers(mut self, n: usize) -> Self {
        assert!(n >= 1, "prop: workers(0) runs nothing");
        self.workers = n;
        self
    }

    /// Makes the run read none of the `DOKIMI_ASSERT_PROP_*` variables, for a
    /// test that runs a property to check something other than a subject.
    pub fn hermetic(mut self) -> Self {
        self.hermetic = true;
        self
    }

    /// Runs each run of the body inside `wrapper`, which calls the run once
    /// inside an environment that it chooses.
    ///
    /// The wrapper receives each case, each shrink candidate and each replay.
    /// The run returns after the body and its cleanups end, and it ends no
    /// test, so a wrapper cannot change the verdict. A wrapper that does not
    /// call the run, calls it a second time or panics ends the call with a
    /// fault. On more than one worker, the wrapper runs on more than one
    /// thread at once.
    pub fn around(mut self, wrapper: impl Fn(&mut dyn FnMut()) + Send + Sync + 'static) -> Self {
        self.around = Some(Arc::new(wrapper));
        self
    }

    /// States the draws and the machine steps of a case that runs first, as
    /// a JSON array of entries: `{"label": …, "value": …}` with a typed
    /// literal for a draw, and `{"step": …}` for a step.
    pub fn draws(mut self, entries: &str) -> Self {
        self.draws = Some(entries.to_owned());
        self
    }

    /// States the generator of a property form's input, in place of the
    /// generator of its type.
    pub fn using(mut self, generator: &Generator<T>) -> Self {
        self.using = Some(generator.clone());
        self
    }

    /// States one case of a property form that runs first: one value for
    /// each argument that the form generates, in order.
    pub fn example(mut self, values: impl IntoIterator<Item = T>) -> Self {
        self.examples.push(values.into_iter().collect());
        self
    }

    /// States one case of a property form over one input for each value,
    /// which run first, in order.
    pub fn examples(mut self, values: impl IntoIterator<Item = T>) -> Self {
        self.examples
            .extend(values.into_iter().map(|value| vec![value]));
        self
    }
}

/// Returns the settings with the valid cases that the run aims for.
///
/// # Panics
///
/// Panics for `n` of 0.
pub fn cases<T>(n: usize) -> Options<T> {
    Options::default().cases(n)
}

/// Returns the settings with the seed of the run.
pub fn seed<T>(seed: u64) -> Options<T> {
    Options::default().seed(seed)
}

/// Returns the settings with a replay token.
pub fn replay<T>(token: &str) -> Options<T> {
    Options::default().replay(token)
}

/// Returns the settings with a coverage requirement.
///
/// # Panics
///
/// Panics for a share outside `[0, 1]`.
pub fn require<T>(label: &str, share: f64) -> Options<T> {
    Options::default().require(label, share)
}

/// Returns the settings with the runs that shrinking may spend.
pub fn shrink<T>(runs: usize) -> Options<T> {
    Options::default().shrink(runs)
}

/// Returns the settings with the time that shrinking may take.
pub fn shrink_time<T>(limit: Duration) -> Options<T> {
    Options::default().shrink_time(limit)
}

/// Returns the settings with the most choices of a case.
///
/// # Panics
///
/// Panics for `n` of 0.
pub fn max_choices<T>(n: usize) -> Options<T> {
    Options::default().max_choices(n)
}

/// Returns the settings with the directory of the store.
pub fn store<T>(dir: impl AsRef<Path>) -> Options<T> {
    Options::default().store(dir)
}

/// Returns the settings with the explanation of a counterexample on or off.
pub fn explain<T>(enabled: bool) -> Options<T> {
    Options::default().explain(enabled)
}

/// Returns the settings with the cases that run at once.
///
/// # Panics
///
/// Panics for `n` of 0.
pub fn workers<T>(n: usize) -> Options<T> {
    Options::default().workers(n)
}

/// Returns the settings of a run that reads no variable of the environment.
pub fn hermetic<T>() -> Options<T> {
    Options::default().hermetic()
}

/// Returns the settings with a wrapper of each run of the body.
pub fn around<T>(wrapper: impl Fn(&mut dyn FnMut()) + Send + Sync + 'static) -> Options<T> {
    Options::default().around(wrapper)
}

/// Returns the settings with the draws and steps of a case that runs first.
pub fn draws<T>(entries: &str) -> Options<T> {
    Options::default().draws(entries)
}

/// Returns the settings with the generator of a property form's input.
pub fn using<T>(generator: &Generator<T>) -> Options<T> {
    Options::default().using(generator)
}

/// Returns the settings with one case of a property form.
pub fn example<T>(values: impl IntoIterator<Item = T>) -> Options<T> {
    Options::default().example(values)
}

/// Returns the settings with one case of a property form per value.
pub fn examples<T>(values: impl IntoIterator<Item = T>) -> Options<T> {
    Options::default().examples(values)
}
