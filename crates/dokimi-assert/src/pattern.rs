//! The portable pattern subset: what the regular expression engines of every
//! target language read the same way.
//!
//! The subset contains:
//!
//! - A literal: any character but the metacharacters `\ . ^ $ | ? * + ( ) [ ] { }`,
//!   or a metacharacter after a backslash.
//! - `.`: any character but the line terminators `\n`, `\r`, U+0085, U+2028
//!   and U+2029.
//! - `\d`, `\w` and `\s`: the ASCII digits, the ASCII letters with the digits
//!   and the underscore, and space with `\t`, `\n`, `\f` and `\r`.
//! - A class `[...]` of characters, ranges and those three escapes, negated
//!   by a leading `^`. Inside a class, `\`, `[`, `]` and a hyphen that forms no
//!   range take a backslash, and a hyphen may stand first or last. A class
//!   may not contain `&&`, `--`, `||` or `~~`.
//! - A group, `(...)` or `(?:...)`, nested at most 100 deep, and alternation
//!   with `|`.
//! - The quantifiers `*`, `+`, `?`, `{m}`, `{m,}` and `{m,n}`, with counts of
//!   at most 1,000 and without a leading zero, and never two in a row. Along
//!   every chain of nested quantifiers, the counts multiply to at most
//!   1,000: each count the upper one, or the lower one when there is no upper
//!   one, and a count of 0 counted as 1.
//! - `^` as the first character of the pattern and `$` as its last.
//!
//! The library parses the subset itself, because two engines that parse a
//! pattern differently would decode different strings from the same choices.

use crate::alphabet;
use regex::Regex;
use std::fmt::{self, Display};

/// The characters that are literal only after a backslash.
const METACHARACTERS: &str = "\\.^$|?*+()[]{}";

/// Pairs that a class may not contain. Java reads `&&` as an intersection,
/// and other engines reserve the rest for set operations.
const RESERVED_PAIRS: [&str; 4] = ["&&", "--", "||", "~~"];

/// The characters that `.` leaves out, because some engine's `.` does not
/// match them.
const LINE_TERMINATORS: &str = "\n\r\u{85}\u{2028}\u{2029}";

const DIGITS: &str = "0123456789";
const WORD: &str = "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ_";
const SPACE: &str = " \t\n\u{c}\r";

/// The largest count of a quantifier, and the largest product of the counts
/// of nested quantifiers. RE2 refuses a larger one of either.
const MAX_COUNT: usize = 1000;

/// The deepest that groups may nest. Python's engine refuses a pattern whose
/// groups nest 495 deep.
const MAX_DEPTH: usize = 100;

/// A pattern outside the portable subset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PatternError {
    /// The pattern.
    pattern: String,
    /// The index of the character at which the parser refused the pattern,
    /// or `None` for a pattern that the regular expression engine refused.
    at: Option<usize>,
    /// Why the subset refuses the pattern.
    reason: String,
}

impl Display for PatternError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "pattern {:?}", self.pattern)?;
        if let Some(at) = self.at {
            write!(f, " at {at}")?;
        }
        write!(f, ": {}", self.reason)
    }
}

impl std::error::Error for PatternError {}

/// The repetitions of a quantified piece: the fewest, and the most when the
/// quantifier states one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Sizes {
    pub(crate) min: usize,
    pub(crate) max: Option<usize>,
}

/// One character of a set, chosen by an index over its members in the order
/// of the default alphabet, so index 0 is the simplest member.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Class {
    /// Inclusive intervals of indices into the default alphabet.
    pub(crate) intervals: Vec<(u32, u32)>,
    /// The number of members.
    pub(crate) size: u32,
}

/// A parsed piece of a pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Node {
    /// One character, which makes no choice.
    Literal(char),
    /// One member of a set.
    Class(Class),
    /// Pieces one after another.
    Sequence(Vec<Node>),
    /// Two or more branches.
    Alternation(Vec<Node>),
    /// A quantified piece.
    Repeat(Box<Node>, Sizes),
}

/// Returns the largest product of quantifier counts along a path through
/// `node`.
fn weight(node: &Node) -> usize {
    match node {
        Node::Repeat(item, sizes) => sizes
            .max
            .unwrap_or(sizes.min)
            .max(1)
            .saturating_mul(weight(item)),
        Node::Sequence(items) => items.iter().map(weight).max().unwrap_or(1),
        Node::Alternation(branches) => branches.iter().map(weight).max().unwrap_or(1),
        _ => 1,
    }
}

fn members(chars: &str) -> Vec<(u32, u32)> {
    chars
        .chars()
        .map(|c| {
            let at = alphabet::index(c);
            (at, at)
        })
        .collect()
}

/// Returns the indices of the default alphabet outside `intervals`.
fn complement(intervals: Vec<(u32, u32)>) -> Vec<(u32, u32)> {
    let mut gaps = Vec::new();
    let mut start = 0;
    for (first, last) in alphabet::merge(intervals) {
        if first > start {
            gaps.push((start, first - 1));
        }
        start = last + 1;
    }
    if start < alphabet::SIZE {
        gaps.push((start, alphabet::SIZE - 1));
    }
    gaps
}

/// Returns the class of the merged intervals, or `None` when it has no member.
fn class_of(intervals: Vec<(u32, u32)>) -> Option<Class> {
    let intervals = alphabet::merge(intervals);
    let size = intervals.iter().map(|(start, end)| end - start + 1).sum();
    (size > 0).then_some(Class { intervals, size })
}

/// Returns the class that `.` decodes from.
fn dot() -> Class {
    // The line terminators leave most of the alphabet, so the class has members.
    class_of(complement(members(LINE_TERMINATORS))).unwrap_or(Class {
        intervals: Vec::new(),
        size: 0,
    })
}

/// Returns the class of a shorthand escape: `\d`, `\w` or `\s`.
fn shorthand(escaped: char) -> Option<&'static str> {
    match escaped {
        'd' => Some(DIGITS),
        'w' => Some(WORD),
        's' => Some(SPACE),
        _ => None,
    }
}

/// Returns the node that a pattern of the portable subset parses to.
pub(crate) fn parse(text: &str) -> Result<Node, PatternError> {
    Parser {
        text,
        chars: text.chars().collect(),
        at: 0,
        depth: 0,
    }
    .pattern()
}

/// Returns the regular expression that matches text as the subset reads the
/// pattern: anywhere in the text, with `$` at the end of the text alone.
pub(crate) fn compile(text: &str) -> Result<Regex, PatternError> {
    parse(text)?;
    // The subset escapes ASCII characters alone, so the character after a
    // backslash is the whole escaped character. An unescaped [ opens a class
    // and an unescaped ] closes it.
    let mut source = String::with_capacity(text.len() + 16);
    let mut in_class = false;
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        match c {
            // The parser refuses a pattern that ends in a backslash.
            '\\' => match chars.next().unwrap_or_default() {
                'd' => source.push_str(if in_class { "0-9" } else { "[0-9]" }),
                'w' => source.push_str(if in_class {
                    "0-9A-Za-z_"
                } else {
                    "[0-9A-Za-z_]"
                }),
                's' => source.push_str(if in_class {
                    " \\t\\n\\x0C\\r"
                } else {
                    "[ \\t\\n\\x0C\\r]"
                }),
                escaped => {
                    source.push('\\');
                    source.push(escaped);
                }
            },
            '.' if !in_class => source.push_str("[^\\n\\r\\x{85}\\x{2028}\\x{2029}]"),
            '[' | ']' => {
                in_class = c == '[';
                source.push(c);
            }
            // regex reads # and whitespace literally without the flag x, and
            // reads & and ~ literally outside a set operation, which the
            // subset refuses.
            c => source.push(c),
        }
    }
    Regex::new(&source).map_err(|refused| PatternError {
        pattern: text.to_owned(),
        at: None,
        reason: refused.to_string(),
    })
}

/// A recursive-descent parser over one pattern.
struct Parser<'t> {
    text: &'t str,
    chars: Vec<char>,
    at: usize,
    depth: usize,
}

impl Parser<'_> {
    fn fail(&self, what: impl Display) -> PatternError {
        PatternError {
            pattern: self.text.to_owned(),
            at: Some(self.at),
            reason: what.to_string(),
        }
    }

    fn peek(&self, ahead: usize) -> Option<char> {
        self.chars.get(self.at + ahead).copied()
    }

    fn next(&mut self, missing: &str) -> Result<char, PatternError> {
        let Some(c) = self.peek(0) else {
            return Err(self.fail(missing));
        };
        self.at += 1;
        Ok(c)
    }

    fn at_closing_anchor(&self) -> bool {
        self.peek(0) == Some('$') && self.at + 1 == self.chars.len()
    }

    fn pattern(mut self) -> Result<Node, PatternError> {
        if self.peek(0) == Some('^') {
            self.at += 1;
        }
        let node = self.alternation()?;
        if self.at_closing_anchor() {
            self.at += 1;
        }
        if let Some(c) = self.peek(0) {
            return Err(self.fail(format_args!("{c:?} is not expected")));
        }
        Ok(node)
    }

    fn alternation(&mut self) -> Result<Node, PatternError> {
        let mut branches = vec![self.sequence()?];
        while self.peek(0) == Some('|') {
            self.at += 1;
            branches.push(self.sequence()?);
        }
        if branches.len() == 1 {
            return Ok(branches.remove(0));
        }
        Ok(Node::Alternation(branches))
    }

    /// Parses pieces up to a `|`, a `)`, the closing anchor or the end. A `$`
    /// that ends the pattern inside a group stops the sequence, and the group
    /// then fails because no `)` closes it.
    fn sequence(&mut self) -> Result<Node, PatternError> {
        let mut items = Vec::new();
        while !matches!(self.peek(0), None | Some('|' | ')')) {
            if self.at_closing_anchor() {
                break;
            }
            items.push(self.quantified()?);
        }
        if items.len() == 1 {
            return Ok(items.remove(0));
        }
        Ok(Node::Sequence(items))
    }

    /// Parses an atom and its quantifier. A second quantifier parses as an
    /// atom, and fails as an unescaped metacharacter.
    fn quantified(&mut self) -> Result<Node, PatternError> {
        let atom = self.atom()?;
        if !matches!(self.peek(0), Some('*' | '+' | '?' | '{')) {
            return Ok(atom);
        }
        let repeat = Node::Repeat(Box::new(atom), self.quantifier()?);
        let weight = weight(&repeat);
        if weight > MAX_COUNT {
            return Err(self.fail(format_args!(
                "nested counts multiply to {weight}, above {MAX_COUNT}"
            )));
        }
        Ok(repeat)
    }

    fn quantifier(&mut self) -> Result<Sizes, PatternError> {
        match self.next("a quantifier is missing")? {
            '*' => return Ok(Sizes { min: 0, max: None }),
            '+' => return Ok(Sizes { min: 1, max: None }),
            '?' => {
                return Ok(Sizes {
                    min: 0,
                    max: Some(1),
                });
            }
            _ => {}
        }
        let low = self.count()?;
        let mut high = Some(low);
        if self.peek(0) == Some(',') {
            self.at += 1;
            high = if self.peek(0) == Some('}') {
                None
            } else {
                Some(self.count()?)
            };
        }
        if self.next("a count is not closed")? != '}' {
            return Err(self.fail("a count is not closed by }"));
        }
        if let Some(high) = high {
            if high < low {
                return Err(self.fail(format_args!("the count {{{low},{high}}} runs backwards")));
            }
        }
        Ok(Sizes {
            min: low,
            max: high,
        })
    }

    /// Parses the digits of a count. RE2 reads a count with a leading zero,
    /// such as `{007}`, as literal text, so a count of two or more digits does
    /// not start with 0.
    fn count(&mut self) -> Result<usize, PatternError> {
        let start = self.at;
        while self.peek(0).is_some_and(|c| c.is_ascii_digit()) {
            self.at += 1;
        }
        if start == self.at {
            return Err(self.fail("a count has no digits"));
        }
        let digits: String = self.chars[start..self.at].iter().collect();
        if digits.len() > 1 && digits.starts_with('0') {
            return Err(self.fail(format_args!("the count {digits} has a leading zero")));
        }
        let count = digits.parse::<usize>().unwrap_or(usize::MAX);
        if count > MAX_COUNT {
            return Err(self.fail(format_args!("the count {count} is above {MAX_COUNT}")));
        }
        Ok(count)
    }

    fn atom(&mut self) -> Result<Node, PatternError> {
        let c = self.next("the pattern ends where a character belongs")?;
        match c {
            '(' => self.group(),
            '[' => self.class().map(Node::Class),
            '.' => Ok(Node::Class(dot())),
            '\\' => {
                let escaped = self.next("the pattern ends with a backslash")?;
                if let Some(chars) = shorthand(escaped) {
                    return Ok(Node::Class(class_of(members(chars)).unwrap_or_else(dot)));
                }
                if !METACHARACTERS.contains(escaped) {
                    return Err(
                        self.fail(format_args!("\\{escaped} is not in the portable subset"))
                    );
                }
                Ok(Node::Literal(escaped))
            }
            c if METACHARACTERS.contains(c) => {
                Err(self.fail(format_args!("{c:?} must be escaped here")))
            }
            c => Ok(Node::Literal(c)),
        }
    }

    fn group(&mut self) -> Result<Node, PatternError> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(self.fail(format_args!("groups nest deeper than {MAX_DEPTH}")));
        }
        if self.peek(0) == Some('?') {
            if self.peek(1) != Some(':') {
                return Err(self.fail("only the (?: group is in the portable subset"));
            }
            self.at += 2;
        }
        let node = self.alternation()?;
        if self.next("a group is not closed")? != ')' {
            return Err(self.fail("a group is not closed by )"));
        }
        self.depth -= 1;
        Ok(node)
    }

    fn class(&mut self) -> Result<Class, PatternError> {
        let negated = self.peek(0) == Some('^');
        if negated {
            self.at += 1;
        }
        let mut found: Vec<(u32, u32)> = Vec::new();
        let mut previous: Option<char> = None;
        let mut first = true;
        loop {
            let c = self.next("a class is not closed")?;
            if c == ']' {
                break;
            }
            if c == '\\' {
                if let Some(chars) = self.peek(0).and_then(shorthand) {
                    self.at += 1;
                    found.extend(members(chars));
                    previous = None;
                    first = false;
                    continue;
                }
            }
            let low = self.class_char(c, previous, first)?;
            previous = if c == '\\' { None } else { Some(c) };
            first = false;
            if self.peek(0) == Some('-') && !matches!(self.peek(1), None | Some(']')) {
                if previous == Some('-') {
                    return Err(self.fail("'--' is reserved inside a class"));
                }
                self.at += 1;
                let end = self.next("a class is not closed")?;
                let high = self.class_char(end, Some('-'), false)?;
                if high < low {
                    return Err(self.fail(format_args!("the range {low}-{high} runs backwards")));
                }
                found.extend(alphabet::indices(u32::from(low), u32::from(high)));
                previous = None;
            } else {
                found.extend(members(low.encode_utf8(&mut [0; 4])));
            }
        }
        if first {
            return Err(self.fail("a class is empty"));
        }
        let members = if negated { complement(found) } else { found };
        class_of(members).ok_or_else(|| self.fail("a class in the pattern has no member"))
    }

    /// Returns the character that a member of a class states, without its
    /// escape.
    fn class_char(
        &mut self,
        c: char,
        previous: Option<char>,
        first: bool,
    ) -> Result<char, PatternError> {
        if c == '\\' {
            let escaped = self.next("a class ends inside an escape")?;
            if !METACHARACTERS.contains(escaped) && escaped != '-' {
                return Err(self.fail(format_args!("\\{escaped} is not in the portable subset")));
            }
            return Ok(escaped);
        }
        if c == '[' {
            return Err(self.fail("[ must be escaped inside a class"));
        }
        if let Some(previous) = previous {
            let pair: String = [previous, c].iter().collect();
            if RESERVED_PAIRS.contains(&pair.as_str()) {
                return Err(self.fail(format_args!("{pair:?} is reserved inside a class")));
            }
        }
        if c == '-' && !first && self.peek(0) != Some(']') {
            return Err(self.fail("a hyphen inside a class must be escaped"));
        }
        Ok(c)
    }
}
