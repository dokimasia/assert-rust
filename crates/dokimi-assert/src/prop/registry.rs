// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The registry of the shapes and the generators of types, the generator of
//! a type, and the shape file of a type.
//!
//! A registration applies to the whole test process. It replaces the shape
//! or the generator of a type wherever the type occurs in a derived input.
//! A registration of a type that a read of the registry has already used
//! panics, so a test that runs in parallel never sees a generator change.

use super::arbitrary::{Arbitrary, Definitions, mismatch};
use super::case::Case;
use super::engine::canonical::same;
use super::engine::case::Stop;
use super::engine::choice::IntegerBounds;
use super::engine::inverse::{CannotInvert, Emitted};
use super::generator::{Generator, Node};
use super::native::Walk;
use super::scope::{Generators, picking};
use super::shape::{Shape, ShapeError, put, reader};
use crate::json::{Json, object};
use crate::value::Value;
use std::any::{Any, TypeId, type_name};
use std::collections::{HashMap, HashSet};
use std::fmt::Debug;
use std::marker::PhantomData;
use std::sync::{Arc, LazyLock, Mutex, MutexGuard, PoisonError};

/// Builds a value of a registered type, boxed, from a native value.
type From = Box<dyn Fn(Value) -> Result<Box<dyn Any>, ShapeError> + Send + Sync>;

/// Returns the native value of a value of a registered type.
type To = Box<dyn Fn(&dyn Any) -> Option<Value> + Send + Sync>;

/// What a registration states for a type.
struct Registration {
    /// The shape of the type at a place where it occurs.
    shape: Box<dyn Fn(&mut Definitions) -> Shape + Send + Sync>,
    from: From,
    to: To,
    /// The registered generator, as the `Generator<T>` that it is.
    generator: Option<Box<dyn Any + Send + Sync>>,
}

/// The registrations of the test process, and the types that a read of the
/// registry has looked up.
#[derive(Default)]
struct Registry {
    registrations: HashMap<TypeId, Arc<Registration>>,
    read: HashSet<TypeId>,
}

static REGISTRY: LazyLock<Mutex<Registry>> = LazyLock::new(Mutex::default);

fn locked() -> MutexGuard<'static, Registry> {
    REGISTRY.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Returns the registration of `T`, and marks `T` as read.
fn registered<T: 'static>() -> Option<Arc<Registration>> {
    let mut registry = locked();
    registry.read.insert(TypeId::of::<T>());
    registry.registrations.get(&TypeId::of::<T>()).cloned()
}

/// Registers `registration` for `T`.
fn admit<T: 'static>(call: &str, registration: Registration) {
    let mut registry = locked();
    let id = TypeId::of::<T>();
    let name = type_name::<T>();
    assert!(
        !registry.registrations.contains_key(&id),
        "prop: {call} registers {name} a second time"
    );
    assert!(
        !registry.read.contains(&id),
        "prop: {call} registers {name} after a read of the registry used its shape"
    );
    registry.registrations.insert(id, Arc::new(registration));
}

/// Returns the conversion of a registered type's values into native values.
fn to_native<T: Arbitrary>() -> To {
    Box::new(|value: &dyn Any| value.downcast_ref::<T>().map(T::to_value))
}

/// Makes `generator` the generator that a draw of [`of`] decodes with for
/// `T`, and that every derived input uses wherever `T` occurs.
///
/// A registered generator has no shape, so [`shape_of`] fails for a type
/// that contains `T`.
///
/// # Panics
///
/// Panics for a second registration of `T`, and for a registration of `T`
/// after a read of the registry used the shape of `T`.
pub fn register<T: Arbitrary + Debug>(generator: Generator<T>) {
    let name = type_name::<T>();
    let external = Generator::from_node(Native::<T> {
        of: generator.clone(),
    });
    admit::<T>(
        "register",
        Registration {
            shape: Box::new(move |definitions| {
                definitions
                    .externals
                    .push((name.to_owned(), external.clone()));
                Shape::reference(name)
            }),
            from: Box::new(|value| T::from_value(value).map(|held| Box::new(held) as Box<dyn Any>)),
            to: to_native::<T>(),
            generator: Some(Box::new(generator)),
        },
    );
}

/// Makes the shape of `T` a literal over `values`, in order, so the first
/// value is the simplest. A generator of `T` returns the registered values
/// themselves.
///
/// # Panics
///
/// Panics for no value, for two values of one typed literal, for a second
/// registration of `T`, and for a registration of `T` after a read of the
/// registry used the shape of `T`.
pub fn register_values<T: Arbitrary + Clone + Send + Sync>(values: impl IntoIterator<Item = T>) {
    let values: Vec<T> = values.into_iter().collect();
    let name = type_name::<T>();
    assert!(
        !values.is_empty(),
        "prop: register_values::<{name}> states no value"
    );
    let literals: Vec<Value> = values.iter().map(T::to_value).collect();
    for (at, literal) in literals.iter().enumerate() {
        let earlier = literals[..at].iter().position(|held| same(held, literal));
        assert!(
            earlier.is_none(),
            "prop: register_values::<{name}>: two values state the typed literal {literal}"
        );
    }
    let shape = Shape::literal(literals.clone());
    admit::<T>(
        "register_values",
        Registration {
            shape: Box::new(move |_| shape.clone()),
            from: Box::new(move |value| {
                literals
                    .iter()
                    .position(|literal| same(literal, &value))
                    .map(|at| Box::new(values[at].clone()) as Box<dyn Any>)
                    .ok_or_else(|| mismatch("a registered value", &value))
            }),
            to: to_native::<T>(),
            generator: None,
        },
    );
}

/// The conversions of one variant of a type whose variants
/// [`register_variants`] states.
type VariantFrom<I> = Box<dyn Fn(Option<Value>) -> Result<I, ShapeError> + Send + Sync>;
type VariantTo<I> = Box<dyn Fn(&I) -> Option<Option<Value>> + Send + Sync>;

/// One variant of a type whose shape [`register_variants`] states: its
/// name, the shape of its payload, and the conversions between the payload
/// and the type.
pub struct Variant<I> {
    name: String,
    shape: Option<fn(&mut Definitions) -> Shape>,
    from: VariantFrom<I>,
    to: VariantTo<I>,
}

impl<I> Debug for Variant<I> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Variant")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

impl<I: 'static> Variant<I> {
    /// Returns the variant named `name` with a payload of type `V`: `into`
    /// makes an `I` of a payload, and `from` returns the payload of an `I` of
    /// the variant, and `None` for an `I` of another variant.
    #[must_use]
    pub fn of<V: Arbitrary>(name: &str, into: fn(V) -> I, from: fn(&I) -> Option<&V>) -> Self {
        Self {
            name: name.to_owned(),
            shape: Some(Definitions::shape::<V>),
            from: Box::new(move |payload| {
                convert_from::<V>(payload.unwrap_or(Value::Null)).map(into)
            }),
            to: Box::new(move |held| from(held).map(|payload| Some(convert_to(payload)))),
        }
    }

    /// Returns the variant named `name` without a payload: `value` makes its
    /// one value, and `is` returns `true` for an `I` of the variant.
    #[must_use]
    pub fn unit(name: &str, value: fn() -> I, is: fn(&I) -> bool) -> Self {
        Self {
            name: name.to_owned(),
            shape: None,
            from: Box::new(move |_| Ok(value())),
            to: Box::new(move |held| is(held).then_some(None)),
        }
    }
}

/// Makes the shape of `I` an enum of `variants`, in order, each with its
/// name and the shape of its payload's type.
///
/// # Panics
///
/// Panics for no variant, for two variants of one name, for a second
/// registration of `I`, and for a registration of `I` after a read of the
/// registry used the shape of `I`.
pub fn register_variants<I: Arbitrary>(variants: impl IntoIterator<Item = Variant<I>>) {
    let variants: Arc<Vec<Variant<I>>> = Arc::new(variants.into_iter().collect());
    let name = type_name::<I>();
    assert!(
        !variants.is_empty(),
        "prop: register_variants::<{name}> states no variant"
    );
    for (at, variant) in variants.iter().enumerate() {
        assert!(
            !variants[..at]
                .iter()
                .any(|earlier| earlier.name == variant.name),
            "prop: register_variants::<{name}> names the variant {:?} twice",
            variant.name
        );
    }
    let (shaping, reading, writing) = (Arc::clone(&variants), Arc::clone(&variants), variants);
    admit::<I>(
        "register_variants",
        Registration {
            shape: Box::new(move |definitions| {
                let listed: Vec<(String, Option<Shape>)> = shaping
                    .iter()
                    .map(|variant| {
                        (
                            variant.name.clone(),
                            variant.shape.map(|shape| shape(definitions)),
                        )
                    })
                    .collect();
                Shape::enumeration(listed)
            }),
            from: Box::new(move |value| {
                let (stated, payload) = variant_parts(value)?;
                let variant = reading
                    .iter()
                    .find(|variant| variant.name == stated)
                    .ok_or_else(|| {
                        ShapeError::new(format!("{stated:?} is no registered variant"))
                    })?;
                (variant.from)(payload).map(|held| Box::new(held) as Box<dyn Any>)
            }),
            to: Box::new(move |value| {
                let held = value.downcast_ref::<I>()?;
                writing.iter().find_map(|variant| {
                    (variant.to)(held).map(|payload| Value::Variant {
                        name: variant.name.clone(),
                        payload: payload.map(Box::new),
                    })
                })
            }),
            generator: None,
        },
    );
}

/// Returns the shape that a registration states for `T` where `T` occurs,
/// or `None` for a type without a registration.
pub(crate) fn shape_of_type<T: 'static>(definitions: &mut Definitions) -> Option<Shape> {
    registered::<T>().map(|registration| (registration.shape)(definitions))
}

/// Builds a value of `T` from a native value, through the registration of
/// `T` where it has one.
///
/// # Errors
///
/// Returns [`ShapeError`] for a value that is no value of `T`.
pub fn convert_from<T: Arbitrary>(value: Value) -> Result<T, ShapeError> {
    match registered::<T>() {
        Some(registration) => (registration.from)(value)?
            .downcast::<T>()
            .map(|held| *held)
            .ok()
            .ok_or(ShapeError::new(
                "the registration built no value of its type",
            )),
        None => T::from_value(value),
    }
}

/// Returns the native value of a value of `T`, through the registration of
/// `T` where it has one.
pub fn convert_to<T: Arbitrary>(value: &T) -> Value {
    match registered::<T>() {
        Some(registration) => (registration.to)(value).unwrap_or_else(|| value.to_value()),
        None => value.to_value(),
    }
}

/// Returns the values of the fields of a record, in the order of `names`.
///
/// # Errors
///
/// Returns [`ShapeError`] for a value that is no record of those fields.
pub fn record_fields(value: Value, names: &[&str]) -> Result<Vec<Value>, ShapeError> {
    let Value::Record(fields) = value else {
        return Err(mismatch("a record", &value));
    };
    let matching = fields.len() == names.len()
        && fields
            .iter()
            .zip(names)
            .all(|((held, _), name)| held == name);
    if !matching {
        return Err(ShapeError::new(format!(
            "the record {} has not the fields {names:?}",
            Value::Record(fields)
        )));
    }
    Ok(fields.into_iter().map(|(_, field)| field).collect())
}

/// Returns the refusal of a variant that a type does not have.
#[must_use]
pub fn unknown_variant(name: &str) -> ShapeError {
    ShapeError::new(format!("{name:?} is no variant of the type"))
}

/// Returns the name and the payload of a variant.
///
/// # Errors
///
/// Returns [`ShapeError`] for a value that is no variant.
pub fn variant_parts(value: Value) -> Result<(String, Option<Value>), ShapeError> {
    match value {
        Value::Variant { name, payload } => Ok((name, payload.map(|payload| *payload))),
        other => Err(mismatch("a variant", &other)),
    }
}

/// The shape file of a type: the document, its definitions, and the refs that
/// name a registered generator.
struct Document {
    json: Json,
    definitions: Vec<(String, Json)>,
    externals: Vec<(String, Generator<Value>)>,
}

/// Returns the shape file of `T`, with `source` naming the type. A read under
/// `using`, the generators of a run, states a ref to each type that the run
/// states.
fn document<T: Arbitrary>(using: Option<Arc<Generators>>) -> Result<Document, ShapeError> {
    let mut definitions = Definitions::under(using);
    let mut json = definitions.shape::<T>().json()?.clone();
    let mut named = Vec::with_capacity(definitions.shapes.len());
    for (name, shape) in &definitions.shapes {
        named.push((name.clone(), shape.json()?.clone()));
    }
    if !named.is_empty() {
        json = put(&json, "definitions", Json::Object(named.clone()));
    }
    let source = object([
        ("language", "rust".into()),
        ("type", type_name::<T>().into()),
    ]);
    Ok(Document {
        json: put(&json, "source", source),
        definitions: named,
        externals: definitions.externals,
    })
}

/// Returns the shape file of `T`: the shape that this library reads from
/// `T`, with its definitions and its source, as JSON text that
/// [`of_shape`](super::of_shape) reads in any language of the definition.
///
/// ```
/// use dokimi_assert::prop;
///
/// let shape = prop::shape_of::<(u8, Option<String>)>()?;
/// assert!(shape.contains(r#""shape": "record""#));
/// # Ok::<(), prop::ShapeError>(())
/// ```
///
/// # Errors
///
/// Returns [`ShapeError`] for a shape that does not read, and for a type
/// that contains a type with a registered generator, which has no shape.
pub fn shape_of<T: Arbitrary>() -> Result<String, ShapeError> {
    let document = document::<T>(None)?;
    if let Some((name, _)) = document.externals.first() {
        return Err(ShapeError::new(format!(
            "{name} has a registered generator, which has no shape"
        )));
    }
    reader::read(&document.json, &[])?;
    Ok(document.json.pretty())
}

/// Returns the generator of `T`: the generator that a registration states
/// for `T`, or else the generator of `T`'s shape.
///
/// A value that the shape's generator decodes becomes a `T` through
/// [`Arbitrary::from_value`]. A value that `from_value` refuses is no value
/// of `T`, so the case is invalid, as a case that [`Case::assume`] rejects
/// is. A type whose values are fewer than its shape's, such as a
/// `jiff::Zoned`, which holds no instant after
/// 9999-12-30T22:00:00.999999999Z, refuses the others.
///
/// ```
/// use dokimi_assert::{Collector, check, prop};
///
/// let t = Collector::new();
/// prop::for_all(&t, "a pair stays a pair", prop::cases(20), |c| {
///     let (a, b) = c.draw(&prop::of::<(u8, bool)>(), "pair");
///     check::is_true(c, u16::from(a) < 256 || b, "a byte is below 256");
/// });
/// ```
///
/// In a run that gives types generators through
/// [`using`](super::Options::using), a draw of the generator decodes with
/// the generator that the run gives `T`. A draw of a type that contains such
/// a type reads the type again with the run's generators in place, once per
/// run, and panics inside its case when that read fails.
///
/// # Panics
///
/// Panics when the shape of `T` does not read, with the reason.
#[must_use]
pub fn of<T: Arbitrary>() -> Generator<T> {
    picking(
        generator_of::<T>()
            .unwrap_or_else(|refused| panic!("prop::of::<{}>: {refused}", type_name::<T>())),
    )
}

/// Returns the generator of `T` in the test process, or the refusal of a
/// shape that does not read.
pub(crate) fn generator_of<T: Arbitrary>() -> Result<Generator<T>, ShapeError> {
    generator_under::<T>(None)
}

/// Returns the generator of `T` that a read under `using`, the generators of
/// a run, returns: the registered generator of `T`, or else the generator of
/// `T`'s shape with a ref to each type that the run states.
pub(crate) fn generator_under<T: Arbitrary>(
    using: Option<Arc<Generators>>,
) -> Result<Generator<T>, ShapeError> {
    if let Some(generator) = registered::<T>().and_then(|registration| {
        registration
            .generator
            .as_ref()
            .and_then(|generator| generator.downcast_ref::<Generator<T>>().cloned())
    }) {
        return Ok(generator);
    }
    let document = document::<T>(using)?;
    let shape = reader::read(&document.json, &document.externals)?;
    Ok(Generator::from_node(Derived::<T> {
        shape,
        root: document.json,
        definitions: document.definitions,
        of: PhantomData,
    }))
}

/// The generator of a type, through the generator of its shape.
struct Derived<T> {
    shape: Generator<Value>,
    root: Json,
    definitions: Vec<(String, Json)>,
    of: PhantomData<fn() -> T>,
}

impl<T> Derived<T> {
    fn walk(&self) -> Walk<'_> {
        Walk {
            definitions: &self.definitions,
        }
    }
}

impl<T: Arbitrary> Node<T> for Derived<T> {
    fn id(&self) -> &'static str {
        self.shape.node().id()
    }

    fn decode(&self, case: &Case<'_>) -> Result<T, Stop> {
        let value = self.shape.decode(case)?;
        self.walk()
            .native(&self.root, value)
            .and_then(convert_from::<T>)
            .map_err(|_| Stop::Rejected)
    }

    fn literal(&self, value: &T) -> Option<Value> {
        self.walk().decoded(&self.root, convert_to(value)).ok()
    }

    fn emit(&self, value: &Value) -> Result<Emitted, CannotInvert> {
        self.shape.emit(value)
    }

    fn integer(&self) -> Option<IntegerBounds> {
        self.shape.node().integer()
    }
}

/// A registered or stated generator, whose values a derived input takes as
/// native values.
pub(crate) struct Native<T> {
    pub(crate) of: Generator<T>,
}

impl<T: Arbitrary + Debug> Node<Value> for Native<T> {
    fn id(&self) -> &'static str {
        self.of.node().id()
    }

    fn decode(&self, case: &Case<'_>) -> Result<Value, Stop> {
        self.of.decode(case).map(|held| held.to_value())
    }

    fn emit(&self, value: &Value) -> Result<Emitted, CannotInvert> {
        self.of.emit(value)
    }
}
