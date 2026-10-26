// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! The spec of `prop/scope`: the generators that `using` gives types in one
//! run, and the draws of `of` that pick them.

use crate::helpers::records;
use dokimi_assert::prop::{Arbitrary, Definitions, Generator, Options, Shape, ShapeError};
use dokimi_assert::{Recorder, Value, check, prop};
use std::fmt::Debug;
use std::sync::Mutex;

/// Returns the settings of a hermetic run of 20 cases that gives `U`
/// `generator`.
fn using<U: Arbitrary + Debug>(generator: &Generator<U>) -> Options {
    prop::using(generator)
        .hermetic()
        .seed(1)
        .store("")
        .cases(20)
}

/// Returns the generator of the digits of `u16`.
fn digits() -> Generator<u16> {
    prop::integer(0..=9_u16)
}

/// A count whose shape bounds the shape of `u16` where `u16` occurs, so it
/// does not read when a run gives `u16` a generator.
#[derive(Debug, Clone, PartialEq)]
struct Bounded(u16);

impl Arbitrary for Bounded {
    fn shape(definitions: &mut Definitions) -> Shape {
        definitions.shape::<u16>().min(1)
    }

    fn from_value(value: Value) -> Result<Self, ShapeError> {
        u16::from_value(value).map(Bounded)
    }

    fn to_value(&self) -> Value {
        self.0.to_value()
    }
}

/// A node of a tree. Its own shape generates a plain node, and the stated
/// generators of a run generate leaves and branches.
#[derive(Debug, Clone, PartialEq)]
enum Node {
    Plain,
    Leaf,
    Branch(Vec<Node>),
}

impl Arbitrary for Node {
    fn shape(_: &mut Definitions) -> Shape {
        Shape::boolean()
    }

    fn from_value(value: Value) -> Result<Self, ShapeError> {
        match value {
            Value::Bool(_) => Ok(Node::Plain),
            Value::List(children) => children
                .into_iter()
                .map(Node::from_value)
                .collect::<Result<_, _>>()
                .map(Node::Branch),
            _ => Ok(Node::Leaf),
        }
    }

    fn to_value(&self) -> Value {
        match self {
            Node::Plain => Value::Bool(false),
            Node::Leaf => Value::Null,
            Node::Branch(children) => Value::List(children.iter().map(Node::to_value).collect()),
        }
    }
}

/// A record with a field of a node.
#[derive(Debug, Clone, PartialEq)]
struct Fork(Node);

impl Arbitrary for Fork {
    fn shape(definitions: &mut Definitions) -> Shape {
        Shape::record([("next", definitions.shape::<Node>())])
    }

    fn from_value(value: Value) -> Result<Self, ShapeError> {
        let Value::Record(mut fields) = value else {
            return Err(ShapeError::new("a fork is a record"));
        };
        Node::from_value(fields.remove(0).1).map(Fork)
    }

    fn to_value(&self) -> Value {
        Value::Record(vec![("next".to_owned(), self.0.to_value())])
    }
}

/// A list of nodes. Its own shape generates a plain forest, and the stated
/// generators of a run generate groves.
#[derive(Debug, Clone, PartialEq)]
enum Forest {
    Plain,
    Grove(Vec<Node>),
}

impl Forest {
    /// Returns the nodes of the forest.
    fn nodes(self) -> Vec<Node> {
        match self {
            Forest::Plain => Vec::new(),
            Forest::Grove(nodes) => nodes,
        }
    }
}

impl Arbitrary for Forest {
    fn shape(_: &mut Definitions) -> Shape {
        Shape::boolean()
    }

    fn from_value(value: Value) -> Result<Self, ShapeError> {
        match value {
            Value::List(nodes) => nodes
                .into_iter()
                .map(Node::from_value)
                .collect::<Result<_, _>>()
                .map(Forest::Grove),
            _ => Ok(Forest::Plain),
        }
    }

    fn to_value(&self) -> Value {
        match self {
            Forest::Plain => Value::Bool(false),
            Forest::Grove(nodes) => Value::List(nodes.iter().map(Node::to_value).collect()),
        }
    }
}

/// Returns the generator of a branch whose children a draw of `of` decodes.
fn branches(_: Generator<Node>) -> Generator<Node> {
    prop::of::<Vec<Node>>().map(Node::Branch)
}

/// Returns the values of `of::<Node>()` for the seed 7 in a run that gives
/// `Node` `generator`.
fn grown(generator: &Generator<Node>) -> Vec<Node> {
    prop::values(&prop::of::<Node>(), 7, 100, prop::using(generator))
}

/// Returns the number of nodes on the longest path down from `node`.
fn levels(node: &Node) -> usize {
    match node {
        Node::Branch(children) => 1 + children.iter().map(levels).max().unwrap_or_default(),
        _ => 1,
    }
}

/// Returns the nodes of `node` in the order that a decode chooses them:
/// `node`, then the nodes of each child in order.
fn preorder(node: &Node) -> Vec<&Node> {
    let mut out = vec![node];
    if let Node::Branch(children) = node {
        out.extend(children.iter().flat_map(preorder));
    }
    out
}

/// The debug text of the definitions of the last read of a [`Probe`].
static PROBED: Mutex<String> = Mutex::new(String::new());

/// A count whose read keeps the debug text of its definitions in
/// [`PROBED`].
#[derive(Debug, Clone, PartialEq)]
struct Probe(u16);

impl Arbitrary for Probe {
    fn shape(definitions: &mut Definitions) -> Shape {
        let shape = definitions.shape::<u16>();
        *PROBED.lock().expect("the lock is free") = format!("{definitions:?}");
        shape
    }

    fn from_value(value: Value) -> Result<Self, ShapeError> {
        u16::from_value(value).map(Probe)
    }

    fn to_value(&self) -> Value {
        self.0.to_value()
    }
}

mod using {
    use super::*;

    #[test]
    fn draws_a_stated_type_with_the_stated_generator() {
        let seat = Recorder::new();
        prop::for_all(&seat, "c", using(&digits()), |c| {
            check::is_true(c, c.draw(&prop::of::<u16>(), "n") < 10, "n is a digit");
        });
        assert!(!seat.failed(), "{}", seat.message());
    }

    #[test]
    fn replaces_an_earlier_generator_of_one_type() {
        let seat = Recorder::new();
        let options = using(&prop::just(1_u16)).using(&prop::just(2_u16));
        prop::for_all(&seat, "c", options, |c| {
            check::equal(
                c,
                c.draw(&prop::of::<u16>(), "n"),
                2,
                "n is the later value",
            );
        });
        assert!(!seat.failed(), "{}", seat.message());
    }

    #[test]
    fn generates_each_stated_type_inside_a_type_that_contains_it() {
        let seat = Recorder::new();
        prop::for_all(&seat, "c", using(&digits()), |c| {
            let ns = c.draw(&prop::of::<Vec<u16>>(), "ns");
            check::is_true(c, ns.iter().all(|n| *n < 10), "every element is a digit");
        });
        assert!(!seat.failed(), "{}", seat.message());
    }

    #[test]
    fn draws_its_own_type_inside_the_stated_generator_without_the_setting() {
        let seat = Recorder::new();
        let above = prop::of::<u16>().map(|n| n.saturating_add(100));
        prop::for_all(&seat, "c", using(&above), |c| {
            check::is_true(
                c,
                c.draw(&prop::of::<u16>(), "n") >= 100,
                "n is 100 or more",
            );
        });
        assert!(!seat.failed(), "{}", seat.message());
    }

    #[test]
    fn keeps_the_generators_of_the_other_types_inside_a_stated_generator() {
        let seat = Recorder::new();
        let widened = prop::of::<u16>().map(u32::from);
        prop::for_all(&seat, "c", using(&prop::just(7_u16)).using(&widened), |c| {
            check::equal(c, c.draw(&prop::of::<u32>(), "n"), 7, "n is the stated u16");
        });
        assert!(!seat.failed(), "{}", seat.message());
    }

    #[test]
    fn draws_a_stated_type_with_the_stated_generator_on_each_worker() {
        let seat = Recorder::new();
        prop::for_all(&seat, "c", using(&digits()).workers(2), |c| {
            check::is_true(c, c.draw(&prop::of::<u16>(), "n") < 10, "n is a digit");
        });
        assert!(!seat.failed(), "{}", seat.message());
    }

    #[test]
    fn follows_an_entry_of_draws_through_the_stated_generator() {
        let seat = Recorder::new();
        let seen = Mutex::new(Vec::new());
        let options = using(&digits())
            .draws(r#"[{"label": "n", "value": {"type": "int", "value": 5}}]"#)
            .cases(1);
        prop::for_all(&seat, "c", options, |c| {
            seen.lock()
                .expect("the lock is free")
                .push(c.draw(&prop::of::<u16>(), "n"));
        });
        assert!(!seat.failed(), "{}", seat.message());
        assert_eq!(seen.lock().expect("the lock is free")[0], 5);
    }

    #[test]
    fn explains_the_nearest_passing_value_of_a_stated_integer() {
        let seat = Recorder::new();
        prop::for_all(&seat, "c", using(&digits()), |c| {
            check::is_true(c, c.draw(&prop::of::<u16>(), "n") < 5, "n is below 5");
        });
        let record = &records(&seat)[0];
        let drawn = &record["detail"]["counterexample"][0];
        assert_eq!(
            drawn["value"],
            serde_json::json!({"type": "int", "value": 5})
        );
        assert_eq!(
            drawn["nearest-passing"],
            serde_json::json!({"type": "int", "value": 4})
        );
    }

    #[test]
    fn fails_a_case_whose_type_does_not_read_under_the_run() {
        let seat = Recorder::new();
        prop::for_all(&seat, "c", using(&digits()), |c| {
            let _ = c.draw(&prop::of::<Bounded>(), "bounded");
        });
        let case = seat.failures()[0]
            .case_failure()
            .cloned()
            .expect("the run found a counterexample");
        assert!(
            case.contract().starts_with("prop::of::<"),
            "{}",
            case.contract()
        );
    }

    #[test]
    fn refers_to_each_stated_type_in_the_definitions_of_a_read() {
        let seat = Recorder::new();
        let probe = prop::of::<Probe>();
        prop::for_all(&seat, "c", using(&digits()).cases(1), |c| {
            let _ = c.draw(&probe, "probe");
        });
        let probed = PROBED.lock().expect("the lock is free").clone();
        assert!(
            probed.contains(r#"externals: [("u16", Generator("integer"))]"#),
            "{probed}"
        );
        assert!(probed.contains(r#"using: Some({"u16"})"#), "{probed}");
    }

    #[test]
    fn takes_each_value_of_a_stated_type_in_a_recursive_extension_from_its_position() {
        let nodes = grown(&prop::recursive(prop::just(Node::Leaf), branches).max_leaves(10));
        assert!(
            nodes
                .iter()
                .flat_map(preorder)
                .all(|n| !matches!(n, Node::Plain)),
            "{nodes:?}"
        );
        assert!(nodes.iter().map(levels).max() >= Some(3), "{nodes:?}");
    }

    #[test]
    fn takes_the_field_of_a_stated_type_in_a_recursive_extension_from_its_position() {
        let chains = prop::recursive(prop::just(Node::Leaf), |_| {
            prop::of::<Fork>().map(|fork| Node::Branch(vec![fork.0]))
        });
        let nodes = grown(&chains);
        assert!(nodes.iter().map(levels).max() >= Some(3), "{nodes:?}");
    }

    #[test]
    fn takes_an_optional_stated_type_in_a_recursive_extension_from_its_position() {
        let chains = prop::recursive(prop::just(Node::Leaf), |_| {
            prop::of::<Option<Node>>().map(|next| Node::Branch(next.into_iter().collect()))
        });
        let nodes = grown(&chains);
        assert!(nodes.iter().map(levels).max() >= Some(3), "{nodes:?}");
    }

    #[test]
    fn takes_the_base_at_every_position_of_a_stated_type_past_the_bound() {
        let nodes = grown(&prop::recursive(prop::just(Node::Leaf), branches).max_leaves(2));
        for node in &nodes {
            let order = preorder(node);
            let leaves: Vec<usize> = order
                .iter()
                .enumerate()
                .filter(|(_, n)| matches!(n, Node::Leaf))
                .map(|(at, _)| at)
                .collect();
            let past = leaves.get(1).map_or(&[][..], |&at| &order[at..]);
            assert!(past.iter().all(|n| matches!(n, Node::Leaf)), "{node:?}");
        }
        let most = nodes
            .iter()
            .map(|node| {
                preorder(node)
                    .into_iter()
                    .filter(|n| matches!(n, Node::Leaf))
                    .count()
            })
            .max();
        assert!(most > Some(2), "{nodes:?}");
    }

    #[test]
    fn draws_a_stated_type_inside_a_recursive_base_without_the_setting() {
        let inner = prop::composite(|c| c.draw(&prop::of::<Node>(), "inner"));
        let nodes = grown(&prop::recursive(inner, branches).max_leaves(10));
        let all: Vec<&Node> = nodes.iter().flat_map(preorder).collect();
        assert!(all.iter().any(|n| matches!(n, Node::Plain)), "{nodes:?}");
        assert!(!all.iter().any(|n| matches!(n, Node::Leaf)), "{nodes:?}");
    }

    #[test]
    fn draws_a_type_without_a_stated_generator_in_a_recursive_extension_from_its_shape() {
        let nodes = prop::recursive(prop::just(Node::Leaf), branches).max_leaves(10);
        let grown = prop::values(&nodes, 7, 100, prop::using(&digits()));
        assert!(
            grown
                .iter()
                .flat_map(preorder)
                .any(|n| matches!(n, Node::Plain)),
            "{grown:?}"
        );
    }

    #[test]
    fn ends_a_cycle_of_two_stated_recursive_generators() {
        let nodes = prop::recursive(prop::just(Node::Leaf), |_| {
            prop::composite(|c| Node::Branch(c.draw(&prop::of::<Forest>(), "forest").nodes()))
        })
        .max_leaves(10);
        let forests = prop::recursive(prop::just(Forest::Grove(Vec::new())), |_| {
            prop::of::<Vec<Node>>().map(Forest::Grove)
        })
        .max_leaves(10);
        let grown = prop::values(
            &prop::of::<Node>(),
            7,
            100,
            prop::using(&nodes).using(&forests),
        );
        assert!(grown.iter().map(levels).max() >= Some(3), "{grown:?}");
    }
}

mod of {
    use super::*;

    #[test]
    fn names_the_kind_of_the_generator_of_its_type_in_its_debug_text() {
        assert_eq!(format!("{:?}", prop::of::<u8>()), r#"Generator("integer")"#);
    }

    #[test]
    fn decodes_with_the_generator_of_the_test_process_outside_a_run() {
        let digits = prop::values(&prop::of::<u16>(), 42, 20, Options::default());
        assert!(digits.iter().any(|n| *n >= 10), "{digits:?}");
    }
}
