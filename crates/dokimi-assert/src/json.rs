// Copyright Dokimasia B.V. 2026
// SPDX-License-Identifier: MIT

//! JSON text, read strictly and written on one line.
//!
//! The library reads and writes JSON for its call records, the entries of a
//! property's store, shape files and typed literals. Every language of the
//! definition reads those files to one verdict, so the reader refuses what a
//! lenient reader accepts: a name that repeats within an object, and a
//! nesting deeper than the caller's limit. The reader reads a `str`, so the
//! text is UTF-8.

use std::fmt::{self, Write as _};

/// A JSON value. An object keeps its members in the order of the text.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Json {
    Null,
    Bool(bool),
    /// A number written without a fraction or an exponent, within the range
    /// of `i128`.
    Int(i128),
    /// Any other number.
    Float(f64),
    String(String),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}

/// Why a text is not one JSON value within a limit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Malformed {
    /// The byte offset at which the reader stopped.
    pub(crate) at: usize,
    /// What the reader found there.
    pub(crate) reason: &'static str,
}

impl fmt::Display for Malformed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at byte {}", self.reason, self.at)
    }
}

impl Json {
    /// Returns the member `name` of an object, and `None` for another value
    /// or an absent member.
    pub(crate) fn get(&self, name: &str) -> Option<&Json> {
        match self {
            Self::Object(members) => members
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value),
            _ => None,
        }
    }

    /// Returns the text of a string.
    pub(crate) fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(text) => Some(text),
            _ => None,
        }
    }

    /// Returns the elements of an array.
    pub(crate) fn as_array(&self) -> Option<&[Json]> {
        match self {
            Self::Array(items) => Some(items),
            _ => None,
        }
    }

    /// Returns the members of an object, in the order of the text.
    pub(crate) fn as_object(&self) -> Option<&[(String, Json)]> {
        match self {
            Self::Object(members) => Some(members),
            _ => None,
        }
    }

    /// Returns a boolean.
    pub(crate) fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(held) => Some(*held),
            _ => None,
        }
    }

    /// Returns an integer.
    pub(crate) fn as_int(&self) -> Option<i128> {
        match self {
            Self::Int(held) => Some(*held),
            _ => None,
        }
    }

    /// Returns a number as a float. An integer converts to the nearest float.
    #[expect(
        clippy::cast_precision_loss,
        reason = "a JSON integer converts to the nearest float, as a JSON reader of floats reads it"
    )]
    pub(crate) fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Int(held) => Some(*held as f64),
            Self::Float(held) => Some(*held),
            _ => None,
        }
    }

    /// Returns the levels of arrays and objects that the value nests. A
    /// scalar nests no level, and an empty array nests one.
    pub(crate) fn depth(&self) -> usize {
        match self {
            Self::Array(items) => 1 + items.iter().map(Self::depth).max().unwrap_or(0),
            Self::Object(members) => {
                1 + members
                    .iter()
                    .map(|(_, value)| value.depth())
                    .max()
                    .unwrap_or(0)
            }
            _ => 0,
        }
    }
}

/// Builds an object from its members, in order.
pub(crate) fn object<const N: usize>(members: [(&str, Json); N]) -> Json {
    Json::Object(
        members
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value))
            .collect(),
    )
}

impl From<&str> for Json {
    fn from(text: &str) -> Self {
        Self::String(text.to_owned())
    }
}

impl From<String> for Json {
    fn from(text: String) -> Self {
        Self::String(text)
    }
}

impl From<bool> for Json {
    fn from(held: bool) -> Self {
        Self::Bool(held)
    }
}

impl From<usize> for Json {
    fn from(held: usize) -> Self {
        Self::Int(i128::try_from(held).unwrap_or(i128::MAX))
    }
}

impl From<u64> for Json {
    fn from(held: u64) -> Self {
        Self::Int(i128::from(held))
    }
}

impl From<u32> for Json {
    fn from(held: u32) -> Self {
        Self::Int(i128::from(held))
    }
}

/// Writes the value on one line, with no space between tokens. A float
/// takes the shortest text that reads back to its bits. A float that is
/// not finite has no JSON text, and is written as `null`.
impl fmt::Display for Json {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bool(held) => write!(f, "{held}"),
            Self::Int(held) => write!(f, "{held}"),
            Self::Float(held) if held.is_finite() => write!(f, "{held:?}"),
            Self::Null | Self::Float(_) => f.write_str("null"),
            Self::String(text) => quote(f, text),
            Self::Array(items) => {
                f.write_char('[')?;
                for (at, item) in items.iter().enumerate() {
                    if at > 0 {
                        f.write_char(',')?;
                    }
                    write!(f, "{item}")?;
                }
                f.write_char(']')
            }
            Self::Object(members) => {
                f.write_char('{')?;
                for (at, (name, value)) in members.iter().enumerate() {
                    if at > 0 {
                        f.write_char(',')?;
                    }
                    quote(f, name)?;
                    write!(f, ":{value}")?;
                }
                f.write_char('}')
            }
        }
    }
}

impl Json {
    /// Returns the value written over several lines, each level indented by
    /// two spaces, as JavaScript's `JSON.stringify(value, null, 2)` lays it
    /// out. An empty array and an empty object stay on one line.
    pub(crate) fn pretty(&self) -> String {
        let mut out = String::new();
        self.lay_out(&mut out, 0);
        out
    }

    fn lay_out(&self, out: &mut String, level: usize) {
        let indent = |out: &mut String, level: usize| out.extend(std::iter::repeat_n("  ", level));
        match self {
            Self::Array(items) if !items.is_empty() => {
                out.push('[');
                for (at, item) in items.iter().enumerate() {
                    out.push_str(if at == 0 { "\n" } else { ",\n" });
                    indent(out, level + 1);
                    item.lay_out(out, level + 1);
                }
                out.push('\n');
                indent(out, level);
                out.push(']');
            }
            Self::Object(members) if !members.is_empty() => {
                out.push('{');
                for (at, (name, value)) in members.iter().enumerate() {
                    out.push_str(if at == 0 { "\n" } else { ",\n" });
                    indent(out, level + 1);
                    // Writing to a String cannot fail.
                    let _ = write!(out, "{}: ", Json::from(name.as_str()));
                    value.lay_out(out, level + 1);
                }
                out.push('\n');
                indent(out, level);
                out.push('}');
            }
            scalar => {
                let _ = write!(out, "{scalar}");
            }
        }
    }
}

/// Writes text as a JSON string. A quote, a backslash and a control
/// character take an escape, and every other character is written as it is.
fn quote(f: &mut fmt::Formatter<'_>, text: &str) -> fmt::Result {
    f.write_char('"')?;
    for c in text.chars() {
        match c {
            '"' => f.write_str("\\\"")?,
            '\\' => f.write_str("\\\\")?,
            '\n' => f.write_str("\\n")?,
            '\r' => f.write_str("\\r")?,
            '\t' => f.write_str("\\t")?,
            '\u{8}' => f.write_str("\\b")?,
            '\u{c}' => f.write_str("\\f")?,
            c if u32::from(c) < 0x20 => write!(f, "\\u{:04x}", u32::from(c))?,
            c => f.write_char(c)?,
        }
    }
    f.write_char('"')
}

/// Reads text as one JSON value that nests at most `limit` levels of
/// arrays and objects.
///
/// # Errors
///
/// Returns [`Malformed`] for text that is not one JSON value, for a name
/// that repeats within an object, and for a nesting deeper than `limit`.
pub(crate) fn parse(text: &str, limit: usize) -> Result<Json, Malformed> {
    let mut reader = Reader {
        bytes: text.as_bytes(),
        at: 0,
        limit,
    };
    reader.space();
    let value = reader.value(0)?;
    reader.space();
    if reader.at != reader.bytes.len() {
        return Err(reader.malformed("text after the value"));
    }
    Ok(value)
}

/// A reader over the bytes of one text.
struct Reader<'t> {
    bytes: &'t [u8],
    at: usize,
    limit: usize,
}

impl Reader<'_> {
    fn malformed(&self, reason: &'static str) -> Malformed {
        Malformed {
            at: self.at,
            reason,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }

    fn space(&mut self) {
        while let Some(b' ' | b'\t' | b'\n' | b'\r') = self.peek() {
            self.at += 1;
        }
    }

    fn expect(&mut self, word: &'static str) -> Result<(), Malformed> {
        if self.bytes[self.at..].starts_with(word.as_bytes()) {
            self.at += word.len();
            return Ok(());
        }
        Err(self.malformed("no JSON value"))
    }

    fn value(&mut self, level: usize) -> Result<Json, Malformed> {
        match self.peek() {
            None => Err(self.malformed("the end of the text")),
            Some(b'n') => self.expect("null").map(|()| Json::Null),
            Some(b't') => self.expect("true").map(|()| Json::Bool(true)),
            Some(b'f') => self.expect("false").map(|()| Json::Bool(false)),
            Some(b'"') => self.string().map(Json::String),
            Some(b'[') => self.array(level + 1),
            Some(b'{') => self.object(level + 1),
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(_) => Err(self.malformed("no JSON value")),
        }
    }

    fn array(&mut self, level: usize) -> Result<Json, Malformed> {
        if level > self.limit {
            return Err(self.malformed("a nesting deeper than the limit"));
        }
        self.at += 1;
        let mut items = Vec::new();
        self.space();
        if self.peek() == Some(b']') {
            self.at += 1;
            return Ok(Json::Array(items));
        }
        loop {
            self.space();
            items.push(self.value(level)?);
            self.space();
            match self.peek() {
                Some(b',') => self.at += 1,
                Some(b']') => {
                    self.at += 1;
                    return Ok(Json::Array(items));
                }
                _ => return Err(self.malformed("no comma or bracket after an element")),
            }
        }
    }

    fn object(&mut self, level: usize) -> Result<Json, Malformed> {
        if level > self.limit {
            return Err(self.malformed("a nesting deeper than the limit"));
        }
        self.at += 1;
        let mut members: Vec<(String, Json)> = Vec::new();
        self.space();
        if self.peek() == Some(b'}') {
            self.at += 1;
            return Ok(Json::Object(members));
        }
        loop {
            self.space();
            if self.peek() != Some(b'"') {
                return Err(self.malformed("no name of a member"));
            }
            let start = self.at;
            let name = self.string()?;
            if members.iter().any(|(held, _)| *held == name) {
                return Err(Malformed {
                    at: start,
                    reason: "a name that repeats within an object",
                });
            }
            self.space();
            if self.peek() != Some(b':') {
                return Err(self.malformed("no colon after a name"));
            }
            self.at += 1;
            self.space();
            let value = self.value(level)?;
            members.push((name, value));
            self.space();
            match self.peek() {
                Some(b',') => self.at += 1,
                Some(b'}') => {
                    self.at += 1;
                    return Ok(Json::Object(members));
                }
                _ => return Err(self.malformed("no comma or brace after a member")),
            }
        }
    }

    fn number(&mut self) -> Result<Json, Malformed> {
        let start = self.at;
        if self.peek() == Some(b'-') {
            self.at += 1;
        }
        match self.peek() {
            Some(b'0') => self.at += 1,
            Some(b'1'..=b'9') => self.digits(),
            _ => return Err(self.malformed("no digit in a number")),
        }
        let mut integral = true;
        if self.peek() == Some(b'.') {
            integral = false;
            self.at += 1;
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err(self.malformed("no digit after a decimal point"));
            }
            self.digits();
        }
        if let Some(b'e' | b'E') = self.peek() {
            integral = false;
            self.at += 1;
            if let Some(b'+' | b'-') = self.peek() {
                self.at += 1;
            }
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err(self.malformed("no digit in an exponent"));
            }
            self.digits();
        }
        // The bytes are ASCII digits, signs and points, so they are UTF-8.
        let text = std::str::from_utf8(&self.bytes[start..self.at]).unwrap_or_default();
        // -0 is the negative zero of a float, which no integer states.
        if integral && text != "-0" {
            if let Ok(held) = text.parse::<i128>() {
                return Ok(Json::Int(held));
            }
        }
        // Every number of the grammar reads as a float, a large one as an
        // infinity.
        text.parse::<f64>()
            .map(Json::Float)
            .ok()
            .ok_or(self.malformed("no number"))
    }

    fn digits(&mut self) {
        while let Some(b'0'..=b'9') = self.peek() {
            self.at += 1;
        }
    }

    fn string(&mut self) -> Result<String, Malformed> {
        self.at += 1;
        let mut text = String::new();
        loop {
            let start = self.at;
            while let Some(byte) = self.peek() {
                if byte == b'"' || byte == b'\\' || byte < 0x20 {
                    break;
                }
                self.at += 1;
            }
            // The text is UTF-8, and a run ends before an ASCII byte or at the
            // end of the text, so the run is UTF-8 too.
            text.push_str(std::str::from_utf8(&self.bytes[start..self.at]).unwrap_or_default());
            match self.peek() {
                Some(b'"') => {
                    self.at += 1;
                    return Ok(text);
                }
                Some(b'\\') => {
                    self.at += 1;
                    text.push(self.escape()?);
                }
                Some(_) => return Err(self.malformed("a control character in a string")),
                None => return Err(self.malformed("a string without its closing quote")),
            }
        }
    }

    fn escape(&mut self) -> Result<char, Malformed> {
        let c = match self.peek() {
            Some(b'"') => '"',
            Some(b'\\') => '\\',
            Some(b'/') => '/',
            Some(b'b') => '\u{8}',
            Some(b'f') => '\u{c}',
            Some(b'n') => '\n',
            Some(b'r') => '\r',
            Some(b't') => '\t',
            Some(b'u') => {
                self.at += 1;
                return self.unicode();
            }
            _ => return Err(self.malformed("an unknown escape")),
        };
        self.at += 1;
        Ok(c)
    }

    fn unicode(&mut self) -> Result<char, Malformed> {
        let high = self.hex4()?;
        if !(0xD800..0xDC00).contains(&high) {
            return char::from_u32(high).ok_or(self.malformed("a lone surrogate"));
        }
        if !self.bytes[self.at..].starts_with(b"\\u") {
            return Err(self.malformed("a lone surrogate"));
        }
        self.at += 2;
        let low = self.hex4()?;
        if !(0xDC00..0xE000).contains(&low) {
            return Err(self.malformed("a lone surrogate"));
        }
        // A pair of surrogates states a code point of a supplementary plane.
        let code = 0x10000 + ((high - 0xD800) << 10) + (low - 0xDC00);
        char::from_u32(code).ok_or(self.malformed("a lone surrogate"))
    }

    fn hex4(&mut self) -> Result<u32, Malformed> {
        let digits = self
            .bytes
            .get(self.at..self.at + 4)
            .filter(|digits| digits.iter().all(u8::is_ascii_hexdigit))
            .ok_or_else(|| self.malformed("an escape without four hexadecimal digits"))?;
        let code = digits.iter().fold(0, |code, digit| {
            // A hexadecimal digit has a value below 16.
            code * 16 + char::from(*digit).to_digit(16).unwrap_or_default()
        });
        self.at += 4;
        Ok(code)
    }
}
