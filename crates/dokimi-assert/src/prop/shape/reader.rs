// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The reader of shape files: the JSON of a shape file, read into the
//! generator that the definition maps its shape to.
//!
//! A key that a shape does not take fails the read, and the failure names
//! where the key is. A constraint never becomes a filter. A definition that
//! can reach itself through refs without passing through a container that
//! can exit has no finite value, and fails the read.

use super::ShapeError;
use super::nodes::{
    Address, Budget, Definitions, Enum, List, Map, Optional, Record, Ref, Root, Target,
};
use super::time::{
    FIRST_DAY, FIRST_SECOND, Instant, LAST_DAY, LAST_SECOND, Local, Pair, SECONDS_PER_DAY, Wall,
    Zoned, date, date_time, fraction_units, per_second, time,
};
use super::zone::zones;
use crate::alphabet;
use crate::json::Json;
use crate::pattern;
use crate::prop::engine::choice::{FloatBounds, IntegerBounds, SequenceBounds, Width};
use crate::prop::engine::collection::Sizes;
use crate::prop::generator::Generator;
use crate::prop::generators::{
    Bytes, Float, Integer, Matching, SampledFrom, Text, WideInteger, boolean,
};
use crate::value::Value;
use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

/// The largest integer that a JSON reader in JavaScript keeps exact.
const SAFE: i128 = (1 << 53) - 1;

/// The largest duration, in nanoseconds: the range of Go's `time.Duration`.
const MAX_DURATION_NANOS: i128 = i64::MAX as i128;

/// The largest offset from UTC, in seconds: 18 hours.
const MAX_OFFSET: i128 = 64_800;

/// The bytes of a UUID, and of an address of each IP version.
const UUID_BYTES: usize = 16;
const IPV4_BYTES: usize = 4;
const IPV6_BYTES: usize = 16;

/// The keys that only the root of a shape file states.
const ROOT_KEYS: [&str; 2] = ["definitions", "source"];

/// Returns the keys that a shape requires, and the keys that it may state,
/// besides `shape`, or `None` for an id that is no shape.
pub(crate) fn keys(kind: &str) -> Option<(&'static [&'static str], &'static [&'static str])> {
    Some(match kind {
        "bool" | "uuid" | "zone" => (&[], &[]),
        "int" => (&["width", "signed"], &["min", "max"]),
        "float" => (&["width"], &["min", "max", "allow_nan", "allow_infinity"]),
        "char" => (&[], &["alphabet"]),
        "string" => (&[], &["min_size", "max_size", "alphabet", "pattern"]),
        "bytes" => (&[], &["min_size", "max_size"]),
        "list" | "set" => (&["of"], &["min_size", "max_size"]),
        "fixed-list" => (&["of", "size"], &[]),
        "map" => (&["key", "of"], &["min_size", "max_size"]),
        "optional" => (&["of"], &[]),
        "record" => (&["fields"], &[]),
        "enum" => (&["variants"], &[]),
        "literal" => (&["values"], &[]),
        "ref" => (&["name"], &[]),
        "ip-address" => (&[], &["version"]),
        "decimal" => (&["scale"], &["min", "max"]),
        "instant" | "time-of-day" | "local-date-time" | "duration" => (&["unit"], &["min", "max"]),
        "date" | "offset" => (&[], &["min", "max"]),
        "zoned-date-time" | "wall-time" => (&["unit"], &[]),
        _ => return None,
    })
}

/// The generators of the refs that name no definition of a shape file: the
/// registered generators of a type's shape, by name.
pub(crate) type Externals = [(String, Generator<Value>)];

/// Returns the generator of a shape file. A ref may name an external
/// generator in place of a definition.
///
/// # Errors
///
/// Returns the refusal of a file that states a shape that the vocabulary does
/// not have, a key that a shape does not take, a parameter that does not
/// read, a ref to no definition, or a definition without a finite value.
pub(crate) fn read(document: &Json, externals: &Externals) -> Result<Generator<Value>, ShapeError> {
    if document.as_object().is_none() {
        return Err(ShapeError::new(format!("{document} is not a shape")));
    }
    Reader::new(document, externals)?.root()
}

/// Returns an integer that a shape states, as a typed literal states an int:
/// a JSON integer within ±(2^53 − 1), and its canonical decimal text beyond.
fn integer(written: &Json) -> Option<i128> {
    match written {
        Json::Int(held) if held.abs() <= SAFE => Some(*held),
        Json::String(text) => text
            .parse::<i128>()
            .ok()
            .filter(|held| held.abs() > SAFE && held.to_string() == *text),
        _ => None,
    }
}

/// Returns an unsigned integer that a shape states, as [`integer`] reads one,
/// up to 2^128 − 1.
fn unsigned(written: &Json) -> Option<u128> {
    match written {
        Json::String(text) => text
            .parse::<u128>()
            .ok()
            .filter(|held| *held > SAFE.unsigned_abs() && held.to_string() == *text),
        other => integer(other).and_then(|held| u128::try_from(held).ok()),
    }
}

/// Returns a float that a shape states: a JSON number, or `NaN`, `Inf` or
/// `-Inf`.
fn number(written: &Json) -> Option<f64> {
    match written {
        Json::String(text) => match text.as_str() {
            "NaN" => Some(f64::NAN),
            "Inf" => Some(f64::INFINITY),
            "-Inf" => Some(f64::NEG_INFINITY),
            _ => None,
        },
        other => other.as_f64(),
    }
}

/// Returns the member `key` of a shape, where the shape states one that is
/// not null.
fn stated<'n>(node: &'n Json, key: &str) -> Option<&'n Json> {
    node.get(key).filter(|value| **value != Json::Null)
}

/// Returns an integer parameter of a shape.
fn int_key(node: &Json, key: &str, at: &str) -> Result<i128, ShapeError> {
    let written = node.get(key).unwrap_or(&Json::Null);
    integer(written)
        .ok_or_else(|| ShapeError::new(format!("{at}.{key} is {written}, not an integer")))
}

/// Returns a count parameter of 0 or more, or `None` when the shape states
/// none.
fn count(node: &Json, key: &str, at: &str) -> Result<Option<usize>, ShapeError> {
    if stated(node, key).is_none() {
        return Ok(None);
    }
    let value = int_key(node, key, at)?;
    usize::try_from(value)
        .map(Some)
        .map_err(|_| ShapeError::new(format!("{at}.{key} is {value}, below 0")))
}

/// Returns `min_size`, 0 by default, and `max_size`, unbounded by default.
fn sizes(node: &Json, at: &str) -> Result<Sizes, ShapeError> {
    let min = count(node, "min_size", at)?.unwrap_or(0);
    let max = count(node, "max_size", at)?;
    Sizes::new(min, max).ok_or_else(|| ShapeError::new(format!("{at}: max_size is below min_size")))
}

/// Returns the units in a second of a time shape's unit.
fn unit(node: &Json, at: &str) -> Result<i128, ShapeError> {
    let written = node.get("unit").unwrap_or(&Json::Null);
    written.as_str().and_then(per_second).ok_or_else(|| {
        ShapeError::new(format!(
            "{at}.unit is {written}, which is none of s, ms, us and ns"
        ))
    })
}

/// Returns the bounds of one choice that a shape's `min` and `max` state,
/// read by `parse`, within `default`.
fn range(
    node: &Json,
    at: &str,
    default: (i128, i128),
    parse: impl Fn(&Json) -> Result<i128, ShapeError>,
) -> Result<(i128, i128), ShapeError> {
    let lo = stated(node, "min").map_or(Ok(default.0), &parse)?;
    let hi = stated(node, "max").map_or(Ok(default.1), &parse)?;
    if !(default.0 <= lo && lo <= hi && hi <= default.1) {
        return Err(ShapeError::new(format!(
            "{at} bounds [{lo}, {hi}] are empty or outside [{}, {}]",
            default.0, default.1
        )));
    }
    Ok((lo, hi))
}

/// Returns the bounds of a two-choice shape that its `min` and `max` state,
/// read by `parse`, within `default`.
fn pair_range(
    node: &Json,
    at: &str,
    default: (Pair, Pair),
    parse: impl Fn(&Json) -> Result<Pair, ShapeError>,
) -> Result<(Pair, Pair), ShapeError> {
    let lo = stated(node, "min").map_or(Ok(default.0), &parse)?;
    let hi = stated(node, "max").map_or(Ok(default.1), &parse)?;
    if !(default.0 <= lo && lo <= hi && hi <= default.1) {
        return Err(ShapeError::new(format!(
            "{at} bounds {lo:?} to {hi:?} are empty or out of range"
        )));
    }
    Ok((lo, hi))
}

/// Returns a JSON integer parameter of a bound, as an int shape states it.
fn integer_bound(at: &str) -> impl Fn(&Json) -> Result<i128, ShapeError> + '_ {
    move |written| {
        integer(written)
            .ok_or_else(|| ShapeError::new(format!("{at} bound {written} is no integer")))
    }
}

/// Returns the generator of one integer choice, whose values are ints.
fn integer_generator(lo: i128, hi: i128) -> Generator<Value> {
    Generator::from_node(Integer::<Value> {
        id: "integer",
        bounds: IntegerBounds::of(lo, hi),
        from: Value::Int,
    })
}

/// Returns the generator of byte strings with a length in `sizes`.
fn bytes_generator(sizes: Sizes) -> Generator<Value> {
    let bounds = SequenceBounds::of(256, sizes.min_size(), sizes.max_size());
    Generator::from_node(Bytes { bounds }).valued()
}

/// Returns the generator of exactly `count` bytes.
fn fixed_bytes(count: usize) -> Generator<Value> {
    bytes_generator(Sizes::new(count, Some(count)).unwrap_or(Sizes::ANY))
}

/// A list of names, each with a value, which a record's fields and an enum's
/// variants state.
type Pairs<'n> = Vec<(&'n str, &'n Json)>;

/// Returns a list of pairs of a name and a value, with distinct names that
/// are not empty.
fn pairs<'n>(node: &'n Json, key: &str, at: &str) -> Result<Pairs<'n>, ShapeError> {
    let items = node
        .get(key)
        .and_then(Json::as_array)
        .filter(|items| !items.is_empty())
        .ok_or_else(|| ShapeError::new(format!("{at}.{key} is not a list of pairs")))?;
    let mut read: Pairs<'n> = Vec::with_capacity(items.len());
    for item in items {
        let Some([Json::String(name), value]) = item.as_array() else {
            return Err(ShapeError::new(format!(
                "{at}.{key} has {item}, not a name and a shape"
            )));
        };
        if name.is_empty() || read.iter().any(|(held, _)| held == name) {
            return Err(ShapeError::new(format!(
                "{at}.{key} names {name:?} twice or not at all"
            )));
        }
        read.push((name.as_str(), value));
    }
    Ok(read)
}

/// One shape file being read: its definitions, which of them are cyclic, and
/// the budget of its values.
struct Reader<'d> {
    document: &'d Json,
    sources: Vec<(&'d str, &'d Json)>,
    externals: &'d Externals,
    cyclic: BTreeSet<String>,
    budget: Budget,
    definitions: Arc<Definitions>,
    built: HashMap<String, Generator<Value>>,
}

impl<'d> Reader<'d> {
    /// Reads the definitions and the source of a shape file, and finds the
    /// definitions that can reach themselves.
    fn new(document: &'d Json, externals: &'d Externals) -> Result<Self, ShapeError> {
        let mut sources = Vec::new();
        if let Some(definitions) = stated(document, "definitions") {
            let members = definitions
                .as_object()
                .ok_or_else(|| ShapeError::new("definitions is not a map of names to shapes"))?;
            for (name, node) in members {
                if node.as_object().is_none() {
                    return Err(ShapeError::new(format!(
                        "definitions.{name} is {node}, not a shape"
                    )));
                }
                sources.push((name.as_str(), node));
            }
        }
        if let Some(source) = stated(document, "source") {
            let named = ["language", "type"]
                .iter()
                .all(|key| source.get(key).and_then(Json::as_str).is_some());
            if !named {
                return Err(ShapeError::new(format!(
                    "source is {source}, not a language and a type"
                )));
            }
        }
        let mut reader = Self {
            document,
            sources,
            externals,
            cyclic: BTreeSet::new(),
            budget: Budget::new(),
            definitions: Arc::default(),
            built: HashMap::new(),
        };
        reader.cyclic = reader.find_cyclic()?;
        reader.check_finite()?;
        Ok(reader)
    }

    /// Builds the definitions, then the root. A file with a cyclic definition
    /// has a root that counts the budget of each value.
    fn root(mut self) -> Result<Generator<Value>, ShapeError> {
        let mut cyclic = HashMap::new();
        for (name, _) in self.sources.clone() {
            let generator = self.definition(name)?;
            if self.cyclic.contains(name) {
                cyclic.insert(name.to_owned(), generator);
            }
        }
        let document = self.document;
        let root = self.build(document, "shape", true)?;
        if self.cyclic.is_empty() {
            return Ok(root);
        }
        let _ = self.definitions.built.set(cyclic);
        Ok(Generator::from_node(Root {
            node: root,
            budget: self.budget,
            _definitions: self.definitions,
        }))
    }

    /// Returns the generator of a definition, which it builds once.
    fn definition(&mut self, name: &str) -> Result<Generator<Value>, ShapeError> {
        if let Some(generator) = self.built.get(name) {
            return Ok(generator.clone());
        }
        // The reader checked that each ref names a definition or an external.
        let node = self
            .sources
            .iter()
            .find_map(|(held, node)| (*held == name).then_some(*node))
            .expect("a ref that names no external names a definition");
        let generator = self.build(node, &format!("definitions.{name}"), false)?;
        self.built.insert(name.to_owned(), generator.clone());
        Ok(generator)
    }

    /// Returns the name of every ref inside a value of the file, without
    /// following a ref. With `stop_at_exits`, a container that can always
    /// exit is not entered, so only the refs that every value expands remain.
    fn refs(&self, node: &Json, stop_at_exits: bool, found: &mut Vec<String>) {
        match node {
            Json::Array(items) => {
                for item in items {
                    self.refs(item, stop_at_exits, found);
                }
            }
            Json::Object(members) => {
                if node.get("shape").and_then(Json::as_str) == Some("ref") {
                    found.push(
                        node.get("name")
                            .map_or_else(String::new, |name| match name {
                                Json::String(text) => text.clone(),
                                other => other.to_string(),
                            }),
                    );
                    return;
                }
                if stop_at_exits && self.exits(node) {
                    return;
                }
                for (key, value) in members {
                    if !ROOT_KEYS.contains(&key.as_str()) && key != "values" {
                        self.refs(value, stop_at_exits, found);
                    }
                }
            }
            _ => {}
        }
    }

    /// Returns the names of the refs inside a value of the file.
    fn refs_of(&self, node: &Json, stop_at_exits: bool) -> BTreeSet<String> {
        let mut found = Vec::new();
        self.refs(node, stop_at_exits, &mut found);
        found.into_iter().collect()
    }

    /// Returns the definitions that can reach themselves through refs.
    fn find_cyclic(&self) -> Result<BTreeSet<String>, ShapeError> {
        let mut reach: Vec<(String, BTreeSet<String>)> = self
            .sources
            .iter()
            .map(|(name, node)| ((*name).to_owned(), self.refs_of(node, false)))
            .collect();
        let mut named = self.refs_of(self.document, false);
        for (_, targets) in &reach {
            named.extend(targets.iter().cloned());
        }
        let known = |target: &String| {
            self.sources.iter().any(|(name, _)| name == target)
                || self.externals.iter().any(|(name, _)| name == target)
        };
        if let Some(target) = named.iter().find(|target| !known(target)) {
            return Err(ShapeError::new(format!(
                "a ref names {target:?}, which is no definition"
            )));
        }
        let mut changed = true;
        while changed {
            changed = false;
            for at in 0..reach.len() {
                let mut wider = reach[at].1.clone();
                for target in &reach[at].1 {
                    if let Some((_, further)) = reach.iter().find(|(name, _)| name == target) {
                        wider.extend(further.iter().cloned());
                    }
                }
                if wider != reach[at].1 {
                    reach[at].1 = wider;
                    changed = true;
                }
            }
        }
        Ok(reach
            .into_iter()
            .filter(|(name, targets)| targets.contains(name))
            .map(|(name, _)| name)
            .collect())
    }

    /// Returns `true` when a value contains a ref to a cyclic definition.
    fn refers_back(&self, node: &Json) -> bool {
        self.refs_of(node, false)
            .iter()
            .any(|name| self.cyclic.contains(name))
    }

    /// Returns `true` when a container can always take an exit, whatever it
    /// contains: an optional can be absent, a list, a set or a map without a
    /// minimum size can be empty, and an enum can take a variant that does not
    /// refer back.
    fn exits(&self, node: &Json) -> bool {
        match node.get("shape").and_then(Json::as_str) {
            Some("optional") => true,
            Some("list" | "set" | "map") => {
                matches!(stated(node, "min_size"), None | Some(Json::Int(0)))
            }
            Some("enum") => self.exit_variant(node).is_some(),
            _ => false,
        }
    }

    /// Returns the index of an enum's first variant that does not refer back.
    fn exit_variant(&self, node: &Json) -> Option<usize> {
        let variants = node
            .get("variants")
            .and_then(Json::as_array)
            .unwrap_or_default();
        variants
            .iter()
            .position(|variant| match variant.as_array() {
                Some([_, payload]) => !self.refers_back(payload),
                _ => true,
            })
    }

    /// Refuses a definition that reaches itself through refs that every value
    /// expands.
    fn check_finite(&self) -> Result<(), ShapeError> {
        let must: HashMap<&str, BTreeSet<String>> = self
            .sources
            .iter()
            .map(|(name, node)| (*name, self.refs_of(node, true)))
            .collect();
        for start in &self.cyclic {
            let mut seen: BTreeSet<String> = BTreeSet::new();
            let mut frontier: BTreeSet<String> =
                must.get(start.as_str()).cloned().unwrap_or_default();
            while !frontier.is_empty() {
                if frontier.contains(start) {
                    return Err(ShapeError::new(format!(
                        "definitions.{start} has no finite value, because it refers back without an optional, a list, a set, a map or an enum that can exit"
                    )));
                }
                seen.extend(frontier.iter().cloned());
                frontier = frontier
                    .iter()
                    .filter_map(|name| must.get(name.as_str()))
                    .flatten()
                    .filter(|name| !seen.contains(*name))
                    .cloned()
                    .collect();
            }
        }
        Ok(())
    }

    /// Returns the generator of one shape.
    fn build(&mut self, node: &Json, at: &str, root: bool) -> Result<Generator<Value>, ShapeError> {
        let Some(members) = node.as_object() else {
            return Err(ShapeError::new(format!("{at} is {node}, not a shape")));
        };
        let written = node.get("shape").unwrap_or(&Json::Null);
        let Some((kind, (required, optional))) =
            written.as_str().and_then(|kind| Some((kind, keys(kind)?)))
        else {
            return Err(ShapeError::new(format!(
                "{at} is {written}, which is no shape"
            )));
        };
        let given: BTreeSet<&str> = members
            .iter()
            .map(|(key, _)| key.as_str())
            .filter(|key| *key != "shape" && !(root && ROOT_KEYS.contains(key)))
            .collect();
        if let Some(missing) = required.iter().find(|key| !given.contains(*key)) {
            return Err(ShapeError::new(format!(
                "{at} states no {missing}, which the {kind} shape needs"
            )));
        }
        if let Some(extra) = given
            .iter()
            .find(|key| !required.contains(key) && !optional.contains(key))
        {
            return Err(ShapeError::new(format!(
                "{at}.{extra} does not apply to the {kind} shape"
            )));
        }
        self.generator_of(kind, node, at)
    }

    /// Returns the generator of a shape of the id `kind`, whose keys the
    /// shape takes.
    fn generator_of(
        &mut self,
        kind: &str,
        node: &Json,
        at: &str,
    ) -> Result<Generator<Value>, ShapeError> {
        match kind {
            "bool" => Ok(boolean().valued()),
            "int" => int_shape(node, at),
            "float" => float_shape(node, at),
            "char" => char_shape(node, at),
            "string" => string_shape(node, at),
            "bytes" => Ok(bytes_generator(sizes(node, at)?)),
            "list" | "set" | "fixed-list" => self.list_shape(node, kind, at),
            "map" => self.map_shape(node, at),
            "optional" => self.optional_shape(node, at),
            "record" => self.record_shape(node, at),
            "enum" => self.enum_shape(node, at),
            "literal" => literal_shape(node, at),
            "ref" => self.ref_shape(node),
            "uuid" => Ok(fixed_bytes(UUID_BYTES)),
            "ip-address" => address_shape(node, at),
            "decimal" => decimal_shape(node, at),
            "instant" => Ok(Generator::from_node(instant(node, at)?)),
            "date" => date_shape(node, at),
            "time-of-day" => time_of_day_shape(node, at),
            "local-date-time" => Ok(Generator::from_node(local(node, at)?)),
            "duration" => duration_shape(node, at),
            "offset" => {
                let (lo, hi) = range(node, at, (-MAX_OFFSET, MAX_OFFSET), integer_bound(at))?;
                Ok(integer_generator(lo, hi))
            }
            "zone" => Ok(Generator::from_node(SampledFrom {
                values: zones()
                    .iter()
                    .map(|zone| Value::from(zone.name.as_str()))
                    .collect(),
            })),
            "zoned-date-time" => Ok(Generator::from_node(Zoned {
                instant: instant(&only_unit(node), at)?,
            })),
            // keys() admits wall-time as the last id.
            _ => Ok(Generator::from_node(Wall {
                local: local(&only_unit(node), at)?,
            })),
        }
    }

    /// Returns the generator of a map from keys to values.
    fn map_shape(&mut self, node: &Json, at: &str) -> Result<Generator<Value>, ShapeError> {
        let key = self.child(node, "key", &format!("{at}.key"))?;
        let of = self.child(node, "of", &format!("{at}.of"))?;
        Ok(Generator::from_node(Map {
            key,
            of,
            sizes: sizes(node, at)?,
            exits: self.exits(node) && self.refers_back(node),
            budget: self.budget,
        }))
    }

    /// Returns the generator of an optional value.
    fn optional_shape(&mut self, node: &Json, at: &str) -> Result<Generator<Value>, ShapeError> {
        Ok(Generator::from_node(Optional {
            of: self.child(node, "of", &format!("{at}.of"))?,
            exits: self.refers_back(node),
            budget: self.budget,
        }))
    }

    /// Returns the generator of a record of named fields.
    fn record_shape(&mut self, node: &Json, at: &str) -> Result<Generator<Value>, ShapeError> {
        let mut fields = Vec::new();
        for (name, of) in pairs(node, "fields", at)? {
            fields.push((
                name.to_owned(),
                self.build(of, &format!("{at}.{name}"), false)?,
            ));
        }
        Ok(Generator::from_node(Record { fields }))
    }

    /// Returns the generator of an enum of named variants, each with a
    /// payload or with none.
    fn enum_shape(&mut self, node: &Json, at: &str) -> Result<Generator<Value>, ShapeError> {
        let mut variants = Vec::new();
        for (name, of) in pairs(node, "variants", at)? {
            let payload = match of {
                Json::Null => None,
                of => Some(self.build(of, &format!("{at}.{name}"), false)?),
            };
            variants.push((name.to_owned(), payload));
        }
        let exit = if self.refers_back(node) {
            self.exit_variant(node)
        } else {
            None
        };
        Ok(Generator::from_node(Enum {
            variants,
            exit,
            budget: self.budget,
        }))
    }

    /// Returns the generator of a named definition, or the external generator
    /// of the name itself. The reader has refused every ref that names
    /// neither.
    fn ref_shape(&mut self, node: &Json) -> Result<Generator<Value>, ShapeError> {
        let name = node
            .get("name")
            .and_then(Json::as_str)
            .unwrap_or_default()
            .to_owned();
        if let Some((_, external)) = self.externals.iter().find(|(held, _)| *held == name) {
            return Ok(external.clone());
        }
        let target = if self.cyclic.contains(&name) {
            Target::Cyclic(Arc::downgrade(&self.definitions))
        } else {
            Target::Built(self.definition(&name)?)
        };
        Ok(Generator::from_node(Ref {
            name,
            target,
            budget: self.budget,
        }))
    }

    /// Returns the generator of a shape nested in another, under `key`.
    fn child(&mut self, node: &Json, key: &str, at: &str) -> Result<Generator<Value>, ShapeError> {
        let nested = node.get(key).unwrap_or(&Json::Null);
        self.build(nested, at, false)
    }

    /// Returns the generator of a list, a fixed-list or a set.
    fn list_shape(
        &mut self,
        node: &Json,
        kind: &str,
        at: &str,
    ) -> Result<Generator<Value>, ShapeError> {
        let of = self.child(node, "of", &format!("{at}.of"))?;
        let sizes = if kind == "fixed-list" {
            let size = count(node, "size", at)?.ok_or_else(|| {
                ShapeError::new(format!("{at} is a fixed-list and states no size"))
            })?;
            Sizes::new(size, Some(size)).unwrap_or(Sizes::ANY)
        } else {
            sizes(node, at)?
        };
        Ok(Generator::from_node(List {
            of,
            sizes,
            unique: kind == "set",
            exits: kind != "fixed-list" && self.exits(node) && self.refers_back(node),
            budget: self.budget,
        }))
    }
}

/// Returns the generator of one character of the alphabet.
fn char_shape(node: &Json, at: &str) -> Result<Generator<Value>, ShapeError> {
    let (alphabet, size) = alphabet_of(node, at)?;
    let bounds = SequenceBounds::of(size, 1, Some(1));
    Ok(Generator::from_node(Text { alphabet, bounds }).valued())
}

/// Returns the generator of one of the stated typed literals.
fn literal_shape(node: &Json, at: &str) -> Result<Generator<Value>, ShapeError> {
    let values = node
        .get("values")
        .and_then(Json::as_array)
        .filter(|values| !values.is_empty())
        .ok_or_else(|| ShapeError::new(format!("{at}.values is not a list of literals")))?
        .iter()
        .map(|literal| {
            Value::from_literal(literal).map_err(|bad| {
                ShapeError::new(format!("{at}.values has {literal}, which is {bad}"))
            })
        })
        .collect::<Result<_, _>>()?;
    Ok(Generator::from_node(SampledFrom { values }))
}

/// Returns the generator of an IP address of one version or of either.
fn address_shape(node: &Json, at: &str) -> Result<Generator<Value>, ShapeError> {
    match stated(node, "version") {
        None => Ok(Generator::from_node(Address {
            versions: [fixed_bytes(IPV4_BYTES), fixed_bytes(IPV6_BYTES)],
        })),
        Some(Json::Int(4)) => Ok(fixed_bytes(IPV4_BYTES)),
        Some(Json::Int(6)) => Ok(fixed_bytes(IPV6_BYTES)),
        Some(other) => Err(ShapeError::new(format!(
            "{at}.version is {other}, not 4 or 6"
        ))),
    }
}

/// Returns the generator of a date, in days since 1970-01-01.
fn date_shape(node: &Json, at: &str) -> Result<Generator<Value>, ShapeError> {
    let parse = |written: &Json| {
        written
            .as_str()
            .and_then(date)
            .ok_or_else(|| ShapeError::new(format!("{at} bound {written} is no date")))
    };
    let (lo, hi) = range(node, at, (FIRST_DAY, LAST_DAY), parse)?;
    Ok(integer_generator(lo, hi))
}

/// Returns the generator of a time of day, in units since midnight.
fn time_of_day_shape(node: &Json, at: &str) -> Result<Generator<Value>, ShapeError> {
    let per = unit(node, at)?;
    let parse = |written: &Json| {
        let (seconds, fraction) = written
            .as_str()
            .and_then(time)
            .ok_or_else(|| ShapeError::new(format!("{at} bound {written} is no time of day")))?;
        let units = fraction_units(fraction, per).ok_or_else(|| {
            ShapeError::new(format!("{at} bound {written} is finer than its unit"))
        })?;
        Ok(seconds * per + units)
    };
    let (lo, hi) = range(node, at, (0, SECONDS_PER_DAY * per - 1), parse)?;
    Ok(integer_generator(lo, hi))
}

/// Returns the generator of a duration, in units.
fn duration_shape(node: &Json, at: &str) -> Result<Generator<Value>, ShapeError> {
    let largest = MAX_DURATION_NANOS / (1_000_000_000 / unit(node, at)?);
    let (lo, hi) = range(node, at, (-largest, largest), integer_bound(at))?;
    Ok(integer_generator(lo, hi))
}

/// Returns a shape with its `unit` alone, as the instant of a zoned date and
/// time and the local date and time of a wall time take it.
fn only_unit(node: &Json) -> Json {
    Json::Object(vec![(
        "unit".to_owned(),
        node.get("unit").cloned().unwrap_or(Json::Null),
    )])
}

/// Returns the generator of an int of the stated width and signedness.
fn int_shape(node: &Json, at: &str) -> Result<Generator<Value>, ShapeError> {
    let width = int_key(node, "width", at)?;
    let signed = node.get("signed").and_then(Json::as_bool);
    let (Some(signed), 8 | 16 | 32 | 64 | 128) = (signed, width) else {
        return Err(ShapeError::new(format!(
            "{at} states width {width} and signed {}",
            node.get("signed").unwrap_or(&Json::Null)
        )));
    };
    if width == 128 && !signed {
        let parse = |key: &str, default: u128| {
            stated(node, key).map_or(Ok(default), |written| {
                unsigned(written)
                    .ok_or_else(|| ShapeError::new(format!("{at} bound {written} is no integer")))
            })
        };
        let (lo, hi) = (parse("min", 0)?, parse("max", u128::MAX)?);
        if lo > hi {
            return Err(ShapeError::new(format!(
                "{at} bounds [{lo}, {hi}] are empty"
            )));
        }
        return Ok(Generator::from_node(WideInteger::<u128> { lo, hi }).valued());
    }
    if width == 128 {
        let (lo, hi) = range(node, at, (i128::MIN, i128::MAX), integer_bound(at))?;
        return Ok(Generator::from_node(WideInteger::<i128> { lo, hi }).valued());
    }
    let widest = if signed {
        (-(1 << (width - 1)), (1 << (width - 1)) - 1)
    } else {
        (0, (1 << width) - 1)
    };
    let (lo, hi) = range(node, at, widest, integer_bound(at))?;
    Ok(integer_generator(lo, hi))
}

/// Returns the generator of a float of the stated width.
fn float_shape(node: &Json, at: &str) -> Result<Generator<Value>, ShapeError> {
    let width = match int_key(node, "width", at)? {
        32 => Width::W32,
        64 => Width::W64,
        other => {
            return Err(ShapeError::new(format!(
                "{at}.width is {other}, not 32 or 64"
            )));
        }
    };
    let flag = |key: &str| match node.get(key) {
        None => Ok(false),
        Some(Json::Bool(held)) => Ok(*held),
        Some(other) => Err(ShapeError::new(format!(
            "{at}.{key} is {other}, not a boolean"
        ))),
    };
    let (allow_nan, infinite) = (flag("allow_nan")?, flag("allow_infinity")?);
    let edge = if infinite {
        f64::INFINITY
    } else if width == Width::W32 {
        f64::from(f32::MAX)
    } else {
        f64::MAX
    };
    let bound = |key: &str, default: f64| {
        stated(node, key).map_or(Ok(default), |written| {
            number(written)
                .ok_or_else(|| ShapeError::new(format!("{at}.{key} is {written}, not a number")))
        })
    };
    let (lo, hi) = (bound("min", -edge)?, bound("max", edge)?);
    if infinite && !(lo.is_infinite() || hi.is_infinite()) {
        return Err(ShapeError::new(format!(
            "{at} allows the infinities and bounds out both"
        )));
    }
    let bounds = FloatBounds::new(lo, hi, allow_nan, width)
        .map_err(|bad| ShapeError::new(format!("{at}: {bad}")))?;
    Ok(Generator::from_node(Float::<Value> {
        bounds,
        from: Value::Float,
    }))
}

/// Returns a stated alphabet and its size, or `None` and the size of the
/// default alphabet.
fn alphabet_of(node: &Json, at: &str) -> Result<(Option<Vec<char>>, u64), ShapeError> {
    let Some(written) = stated(node, "alphabet") else {
        return Ok((None, u64::from(alphabet::SIZE)));
    };
    let characters: Vec<char> = written
        .as_str()
        .filter(|text| !text.is_empty())
        .ok_or_else(|| ShapeError::new(format!("{at}.alphabet is {written}, not characters")))?
        .chars()
        .collect();
    if characters
        .iter()
        .enumerate()
        .any(|(index, c)| characters[..index].contains(c))
    {
        return Err(ShapeError::new(format!(
            "{at}.alphabet repeats a character"
        )));
    }
    let size = u64::try_from(characters.len()).unwrap_or(u64::MAX);
    Ok((Some(characters), size))
}

/// Returns the generator of a string: over an alphabet, or matching a
/// pattern.
fn string_shape(node: &Json, at: &str) -> Result<Generator<Value>, ShapeError> {
    if let Some(written) = node.get("pattern") {
        if ["alphabet", "min_size", "max_size"]
            .iter()
            .any(|key| node.get(key).is_some())
        {
            return Err(ShapeError::new(format!(
                "{at} states a pattern and an alphabet or a size"
            )));
        }
        let text = written
            .as_str()
            .ok_or_else(|| ShapeError::new(format!("{at}.pattern is {written}, not a pattern")))?;
        let parsed = pattern::parse(text).map_err(|bad| ShapeError::new(format!("{at}: {bad}")))?;
        return Ok(Generator::from_node(Matching { node: parsed }).valued());
    }
    let (alphabet, size) = alphabet_of(node, at)?;
    let sizes = sizes(node, at)?;
    let bounds = SequenceBounds::of(size, sizes.min_size(), sizes.max_size());
    Ok(Generator::from_node(Text { alphabet, bounds }).valued())
}

/// Returns the unscaled value of a decimal bound at a scale, rounded up for
/// a minimum and down for a maximum, or `None` beyond the 128-bit range.
fn unscaled(text: &str, scale: u32, up: bool) -> Option<i128> {
    let (negative, digits) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    let (whole, fraction) = digits.split_once('.').unwrap_or((digits, ""));
    let decimal = |part: &str| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit());
    if !decimal(whole) || (digits.contains('.') && !decimal(fraction)) {
        return None;
    }
    let places = u32::try_from(fraction.len()).ok()?;
    let mut magnitude: i128 = format!("{whole}{fraction}").parse().ok()?;
    if negative {
        magnitude = -magnitude;
    }
    if places <= scale {
        return magnitude.checked_mul(10_i128.checked_pow(scale - places)?);
    }
    let divisor = 10_i128.checked_pow(places - scale)?;
    let floor = magnitude.div_euclid(divisor);
    Some(if up && magnitude.rem_euclid(divisor) != 0 {
        floor + 1
    } else {
        floor
    })
}

/// Returns the generator of a decimal's unscaled value at the stated scale.
/// A bound with more digits than the scale rounds inward.
fn decimal_shape(node: &Json, at: &str) -> Result<Generator<Value>, ShapeError> {
    let scale = count(node, "scale", at)?
        .ok_or_else(|| ShapeError::new(format!("{at} is a decimal and states no scale")))?;
    let scale =
        u32::try_from(scale).map_err(|_| ShapeError::new(format!("{at}.scale is too large")))?;
    let bound = |key: &str, up: bool, default: i128| {
        stated(node, key).map_or(Ok(default), |written| {
            written
                .as_str()
                .and_then(|text| unscaled(text, scale, up))
                .ok_or_else(|| ShapeError::new(format!("{at}.{key} is {written}, not a decimal")))
        })
    };
    let (lo, hi) = (
        bound("min", true, i128::from(i64::MIN))?,
        bound("max", false, i128::from(i64::MAX))?,
    );
    let unscaled_node = Json::Object(vec![
        ("min".to_owned(), Json::Int(lo)),
        ("max".to_owned(), Json::Int(hi)),
    ]);
    let (lo, hi) = range(
        &unscaled_node,
        at,
        (i128::from(i64::MIN), i128::from(i64::MAX)),
        |written: &Json| Ok(written.as_int().unwrap_or_default()),
    )?;
    Ok(integer_generator(lo, hi))
}

/// Returns the generator of an instant at the stated unit.
fn instant(node: &Json, at: &str) -> Result<Instant, ShapeError> {
    let per = unit(node, at)?;
    let parse = |written: &Json| {
        let stated = written
            .as_str()
            .and_then(date_time)
            .filter(|stated| stated.utc)
            .ok_or_else(|| ShapeError::new(format!("{at} bound {written} is no instant in UTC")))?;
        let units = fraction_units(stated.fraction, per).ok_or_else(|| {
            ShapeError::new(format!("{at} bound {written} is finer than its unit"))
        })?;
        Ok((stated.day * SECONDS_PER_DAY + stated.seconds, units))
    };
    let default = ((FIRST_SECOND, 0), (LAST_SECOND, per - 1));
    let (lo, hi) = pair_range(node, at, default, parse)?;
    Ok(Instant { per, lo, hi })
}

/// Returns the generator of a date and a time of day without a zone.
fn local(node: &Json, at: &str) -> Result<Local, ShapeError> {
    let per = unit(node, at)?;
    let parse = |written: &Json| {
        let stated = written
            .as_str()
            .and_then(date_time)
            .filter(|stated| !stated.utc)
            .ok_or_else(|| {
                ShapeError::new(format!("{at} bound {written} is no local date and time"))
            })?;
        let units = fraction_units(stated.fraction, per).ok_or_else(|| {
            ShapeError::new(format!("{at} bound {written} is finer than its unit"))
        })?;
        Ok((stated.day, stated.seconds * per + units))
    };
    let default = ((FIRST_DAY, 0), (LAST_DAY, SECONDS_PER_DAY * per - 1));
    let (lo, hi) = pair_range(node, at, default, parse)?;
    Ok(Local { per, lo, hi })
}
