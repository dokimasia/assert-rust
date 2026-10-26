// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The generators that the option `using` gives types in one run, and the
//! generator of a type that picks from them where it is used.
//!
//! A run sets its generators on the thread of its call and around each run
//! of its body. A draw of `of::<T>()` picks the generator of `T` from the
//! generators of the current thread: the stated generator of a stated type,
//! the read of a type that contains a stated type with the stated generators
//! in place, and the generator of the test process outside a run. A stated
//! generator decodes under the run's other generators, so a draw of its own
//! type inside it picks the generator of the test process. Inside the stated
//! generator of `T`, the extension of a recursive generator of `T` decodes
//! under the generators with its position as the generator of `T`.

use super::arbitrary::Arbitrary;
use super::case::Case;
use super::engine::case::Stop;
use super::engine::choice::IntegerBounds;
use super::engine::inverse::{CannotInvert, Emitted};
use super::generator::{Generator, Node};
use super::registry::{Native, generator_under};
use super::shape::ShapeError;
use crate::value::Value;
use std::any::{Any, TypeId, type_name};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::fmt::{self, Debug};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

fn locked<T>(lock: &Mutex<T>) -> MutexGuard<'_, T> {
    lock.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The generator that `using` states for one type.
trait Stated: Send + Sync {
    /// Returns the name of the type.
    fn name(&self) -> &'static str;

    /// Returns the stated generator, as the `Generator<U>` that it is.
    fn typed(&self) -> &(dyn Any + Send + Sync);

    /// Returns the stated generator as a generator of native values that
    /// decodes under `rest`, the ref that a read of a containing type takes.
    fn external(&self, rest: Arc<Generators>) -> Generator<Value>;

    /// Returns `position`, the generator of the positions of a recursive
    /// generator of the type, as a stated generator of the type.
    fn positioned(&self, position: &(dyn Any + Send + Sync)) -> Arc<dyn Stated>;
}

/// The generator that `using` states for `U`.
struct StatedOf<U> {
    generator: Generator<U>,
}

impl<U: Arbitrary + Debug> Stated for StatedOf<U> {
    fn name(&self) -> &'static str {
        type_name::<U>()
    }

    fn typed(&self) -> &(dyn Any + Send + Sync) {
        &self.generator
    }

    fn external(&self, rest: Arc<Generators>) -> Generator<Value> {
        Generator::from_node(Native {
            of: within(self.generator.clone(), rest),
        })
    }

    fn positioned(&self, position: &(dyn Any + Send + Sync)) -> Arc<dyn Stated> {
        let generator = position
            .downcast_ref::<Generator<U>>()
            .expect("a position of a recursive generator of the type")
            .clone();
        Arc::new(StatedOf { generator })
    }
}

/// The generators that the options `using` of a run state, by type. A later
/// option of a type replaces an earlier one.
#[derive(Clone, Default)]
pub(crate) struct Using(HashMap<TypeId, Arc<dyn Stated>>);

impl Debug for Using {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        names(f, &self.0)
    }
}

/// Writes the names of the stated types, in the order of the names.
fn names(f: &mut fmt::Formatter<'_>, stated: &HashMap<TypeId, Arc<dyn Stated>>) -> fmt::Result {
    let mut listed: Vec<&str> = stated.values().map(|stated| stated.name()).collect();
    listed.sort_unstable();
    f.debug_set().entries(listed).finish()
}

impl Using {
    /// Makes `generator` the generator of `U`.
    pub(crate) fn insert<U: Arbitrary + Debug>(&mut self, generator: &Generator<U>) {
        let stated = StatedOf {
            generator: generator.clone(),
        };
        self.0.insert(TypeId::of::<U>(), Arc::new(stated));
    }

    /// Returns the generators of a run, and `None` for a run that states
    /// none.
    pub(crate) fn generators(&self) -> Option<Arc<Generators>> {
        (!self.0.is_empty()).then(|| Arc::new(Generators::new(self.0.clone())))
    }
}

/// The generators that one run gives types, with the reads that its draws
/// made. Inside the stated generator of a type, the draws read the
/// generators of the run without that type. Inside the extension of a
/// recursive generator of that type, they read the generator of the
/// position as the generator of the type. The methods are safe for
/// concurrent use.
pub(crate) struct Generators {
    /// The generators by type. Each is the last generator of its type that
    /// the run states, or the generator of a position.
    stated: HashMap<TypeId, Arc<dyn Stated>>,
    /// The types whose stated generators decode around the draws.
    inside: HashSet<TypeId>,
    /// The generators that the run states, by type.
    run: Arc<HashMap<TypeId, Arc<dyn Stated>>>,
    /// The generators that the draws of `of` picked, by type: of `T`, a
    /// `Generator<T>`.
    picked: Mutex<HashMap<TypeId, Box<dyn Any + Send + Sync>>>,
    /// The generators without each stated type, by type.
    without: Mutex<HashMap<TypeId, Arc<Generators>>>,
    /// The generators inside the extensions of recursive generators, by the
    /// number of the recursive generator.
    extended: Mutex<HashMap<usize, Arc<Generators>>>,
}

impl Debug for Generators {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        names(f, &self.stated)
    }
}

impl Generators {
    fn new(stated: HashMap<TypeId, Arc<dyn Stated>>) -> Self {
        let run = Arc::new(stated.clone());
        Self::under(stated, HashSet::new(), run)
    }

    /// Returns the generators of `stated` inside the stated generators of
    /// the types of `inside`, in a run that states `run`.
    fn under(
        stated: HashMap<TypeId, Arc<dyn Stated>>,
        inside: HashSet<TypeId>,
        run: Arc<HashMap<TypeId, Arc<dyn Stated>>>,
    ) -> Self {
        Self {
            stated,
            inside,
            run,
            picked: Mutex::default(),
            without: Mutex::default(),
            extended: Mutex::default(),
        }
    }

    /// Returns the generators inside the stated generator of the type `id`,
    /// once for each type. They are the generators without the one of `id`.
    fn without(&self, id: TypeId) -> Arc<Self> {
        let mut without = locked(&self.without);
        let rest = without.entry(id).or_insert_with(|| {
            let mut rest = self.stated.clone();
            rest.remove(&id);
            let mut inside = self.inside.clone();
            inside.insert(id);
            Arc::new(Self::under(rest, inside, Arc::clone(&self.run)))
        });
        Arc::clone(rest)
    }

    /// Returns the generators inside the extension of the recursive
    /// generator numbered `recursive`, whose positions `position` generates.
    /// Inside the stated generator of `T`, they are these generators with
    /// `position` as the generator of `T`, and `position` decodes under these
    /// generators. Anywhere else the result is `None`.
    fn extending<T: 'static>(
        self: &Arc<Self>,
        recursive: usize,
        position: &Generator<T>,
    ) -> Option<Arc<Self>> {
        let id = TypeId::of::<T>();
        if !self.inside.contains(&id) {
            return None;
        }
        let run = self.run.get(&id)?;
        let mut extended = locked(&self.extended);
        let inner = extended.entry(recursive).or_insert_with(|| {
            let mut generators = self.stated.clone();
            generators.insert(id, run.positioned(position));
            let inner = Self::under(generators, self.inside.clone(), Arc::clone(&self.run));
            locked(&inner.without).insert(id, Arc::clone(self));
            Arc::new(inner)
        });
        Some(Arc::clone(inner))
    }

    /// Returns the generator of native values of `T`, a stated type, which a
    /// read of a type that contains `T` refers to, and `None` for a type
    /// that the run does not state.
    pub(crate) fn external<T: 'static>(&self) -> Option<Generator<Value>> {
        let id = TypeId::of::<T>();
        let stated = self.stated.get(&id)?;
        Some(stated.external(self.without(id)))
    }

    /// Returns the generator of `T` under the generators: the stated
    /// generator of a stated type, which decodes under the generators without
    /// `T`, and otherwise the read of `T` with the stated generators in place.
    /// A property form reads its input this way.
    pub(crate) fn read<T: Arbitrary>(self: &Arc<Self>) -> Result<Generator<T>, ShapeError> {
        let id = TypeId::of::<T>();
        match self.stated.get(&id) {
            Some(stated) => {
                let generator = stated
                    .typed()
                    .downcast_ref::<Generator<T>>()
                    .expect("a stated generator is a generator of its type")
                    .clone();
                Ok(within(generator, self.without(id)))
            }
            None => generator_under::<T>(Some(Arc::clone(self))),
        }
    }

    /// Returns the generator that a draw of `of::<T>()` picks under the
    /// generators. A run reads each type once.
    ///
    /// # Panics
    ///
    /// Panics for a type that does not read with the stated generators in
    /// place, with the message of `of`.
    fn pick<T: Arbitrary>(self: &Arc<Self>) -> Generator<T> {
        let id = TypeId::of::<T>();
        if let Some(picked) = locked(&self.picked).get(&id) {
            return typed(&**picked);
        }
        let generator = self
            .read::<T>()
            .unwrap_or_else(|refused| panic!("prop::of::<{}>: {refused}", type_name::<T>()));
        let mut picked = locked(&self.picked);
        let kept = picked
            .entry(id)
            .or_insert_with(|| -> Box<dyn Any + Send + Sync> { Box::new(generator) });
        typed(&**kept)
    }
}

/// Returns the generator that a box of picked generators keeps for `T`.
fn typed<T: 'static>(picked: &(dyn Any + Send + Sync)) -> Generator<T> {
    picked
        .downcast_ref::<Generator<T>>()
        .expect("a picked generator is a generator of its type")
        .clone()
}

thread_local! {
    /// The generators of the run whose draws decode on this thread.
    static RUN: RefCell<Option<Arc<Generators>>> = const { RefCell::new(None) };
}

/// The generators that a thread had before [`enter`], which the drop of the
/// guard makes current again.
#[must_use = "the generators apply until the guard drops"]
pub(crate) struct Entered {
    previous: Option<Arc<Generators>>,
}

impl Drop for Entered {
    fn drop(&mut self) {
        let previous = self.previous.take();
        RUN.with(|run| *run.borrow_mut() = previous);
    }
}

/// Makes `generators` the generators of the draws on this thread until the
/// guard drops. `None` stands for the generators of the test process.
pub(crate) fn enter(generators: Option<Arc<Generators>>) -> Entered {
    Entered {
        previous: RUN.with(|run| run.replace(generators)),
    }
}

/// Returns the generators of the draws on this thread.
pub(crate) fn current() -> Option<Arc<Generators>> {
    RUN.with(|run| run.borrow().clone())
}

/// Returns the generator that `of::<T>()` returns: it picks the generator of
/// `T` from the generators of the current thread, and `default`, the
/// generator of the test process, outside a run that states one.
pub(crate) fn picking<T: Arbitrary>(default: Generator<T>) -> Generator<T> {
    Generator::from_node(Picking { default })
}

/// Returns a generator that decodes `of` under `rest`.
fn within<T: 'static>(of: Generator<T>, rest: Arc<Generators>) -> Generator<T> {
    Generator::from_node(Within { of, rest })
}

/// Makes the generators of the draws inside the extension of the recursive
/// generator numbered `recursive` the generators of this thread until the
/// guard drops. `position` is the generator of the positions of the
/// extension. Outside the stated generator of `T`, the generators of the
/// thread do not change, and the result is `None`.
pub(crate) fn extending<T: 'static>(recursive: usize, position: &Generator<T>) -> Option<Entered> {
    let inner = current()?.extending(recursive, position)?;
    Some(enter(Some(inner)))
}

/// The generator of a type, which picks its generator where it is used.
struct Picking<T> {
    default: Generator<T>,
}

impl<T: Arbitrary> Picking<T> {
    fn picked(&self) -> Generator<T> {
        match current() {
            Some(run) if !run.stated.is_empty() => run.pick::<T>(),
            _ => self.default.clone(),
        }
    }
}

impl<T: Arbitrary> Node<T> for Picking<T> {
    fn id(&self) -> &'static str {
        self.default.node().id()
    }

    fn decode(&self, case: &Case<'_>) -> Result<T, Stop> {
        self.picked().decode(case)
    }

    fn literal(&self, value: &T) -> Option<Value> {
        self.picked().literal(value)
    }

    fn emit(&self, value: &Value) -> Result<Emitted, CannotInvert> {
        self.picked().emit(value)
    }

    fn integer(&self) -> Option<IntegerBounds> {
        self.picked().node().integer()
    }
}

/// A stated generator, which decodes under the run's other generators.
struct Within<T> {
    of: Generator<T>,
    rest: Arc<Generators>,
}

impl<T: 'static> Within<T> {
    fn entered(&self) -> Entered {
        enter(Some(Arc::clone(&self.rest)))
    }
}

impl<T: 'static> Node<T> for Within<T> {
    fn id(&self) -> &'static str {
        self.of.node().id()
    }

    fn decode(&self, case: &Case<'_>) -> Result<T, Stop> {
        let _run = self.entered();
        self.of.decode(case)
    }

    fn literal(&self, value: &T) -> Option<Value> {
        let _run = self.entered();
        self.of.literal(value)
    }

    fn emit(&self, value: &Value) -> Result<Emitted, CannotInvert> {
        let _run = self.entered();
        self.of.emit(value)
    }

    fn integer(&self) -> Option<IntegerBounds> {
        let _run = self.entered();
        self.of.node().integer()
    }
}
