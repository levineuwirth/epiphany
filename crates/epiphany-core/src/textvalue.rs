//! The s-expression value model of the Text Projection companion, and the
//! [`TextValue`] trait that projects a canonical value into it and parses one
//! back.
//!
//! # The projection is schema-directed
//!
//! `req:textproj:schema-directed`. Shape does not determine type. `()` is the
//! empty sequence *and* the absent option; a bare symbol is a fieldless variant
//! *and* a zero-field struct; a parenthesised list headed by a symbol is a struct
//! *and* a sequence whose first element is a symbol. A reader resolves each by the
//! type it expects, exactly as the binary decoder does.
//!
//! [`Sexp`] therefore carries only what the *lexer* can honestly know. It has no
//! `Bool` variant, because `true` and `false` are spelled exactly like symbols and
//! nothing but the expected type tells them apart; `bool` projects to
//! [`Sexp::Symbol`]. It has no `Ratio` or `Option` variant for the same reason:
//! both are lists.
//!
//! # Strictness is per-site, and that is a finding, not a shortcut
//!
//! `req:textproj:strict-parse` forbids normalizing. The binary decoders enforce
//! the same rule in two layers: a whole-value re-encode-and-compare guard, plus
//! per-site checks for the order-preserving fields that guard is blind to (see
//! `epiphany-bundle/DECISIONS.md`).
//!
//! The text projection was built the same way and **the whole-value layer turned
//! out to be dead here.** A re-project-and-compare guard can only fire when a
//! parse *normalizes*, and in Chapter 5 every constructor that could normalize
//! needed a check that names the fault anyway:
//!
//! * a set or map re-sorts and de-duplicates, so `parse` walks the elements and
//!   rejects the first that does not strictly increase;
//! * `RationalTime::new` reduces, so `parse` compares against `BigRational::new`'s
//!   canonical form *before* constructing;
//! * a catalog id folds to NFC, so `parse` interns and then compares;
//! * `EventArena::insert` re-sorts, so `parse` checks ascending `EventId` itself.
//!
//! Every one of those is mutation-verified live. The remaining validating
//! constructors — `Tempo::new`, `ReferencePitch::new`, `SpellingPrecedence::new`,
//! `EventOrderingDAG::try_new` — *reject* rather than adjust, so an accepted value
//! re-projects to exactly its input and a whole-value guard could never fire. Four
//! such guards were written, mutation-tested, found dead, and removed. A check
//! that cannot fail is worse than no check: it invites weakening the real one.
//!
//! The blind spot the binary form documents still applies and still needs naming:
//! where a `Vec`'s order is constrained (the frozen `Transpose`'s non-decreasing
//! `targets`), only a per-site check can see it. Nothing in Chapter 5 constrains a
//! `Vec` that way; the operation payloads do.

use core::fmt;
use std::collections::{BTreeMap, BTreeSet};

use num_bigint::BigInt;

// ===========================================================================
// The value model.
// ===========================================================================

/// One node of a projected value.
///
/// The five variants are exactly the lexical classes a reader can distinguish
/// without knowing the expected type: a parenthesised list, a symbol (`[a-z]`
/// then `[a-z0-9-]*`), an integer, a byte string (`#x` then lowercase hex pairs),
/// and a quoted string.
///
/// There is deliberately no `Bool`: `true` is lexically a symbol, and only the
/// expected type says otherwise. Likewise a rational is a `List`, and so is an
/// option.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Sexp {
    /// A parenthesised list: a struct, a sequence, a set, a map, an option, or a
    /// rational, according to the expected type.
    List(Vec<Sexp>),
    /// A symbol: a type name, a variant name, or a boolean.
    Symbol(String),
    /// An integer of arbitrary precision. `RationalTime` promotes to a
    /// `BigRational`, so the text's integers cannot be bounded by `i64`.
    Int(BigInt),
    /// A byte string. Rendered `#x` followed by an even number of lowercase hex
    /// digits; the empty byte string is `#x`.
    Bytes(Vec<u8>),
    /// A quoted, escaped string.
    Str(String),
}

impl Sexp {
    /// The symbol `name`.
    pub fn sym(name: &str) -> Sexp {
        Sexp::Symbol(name.to_owned())
    }

    /// An integer.
    pub fn int(value: impl Into<BigInt>) -> Sexp {
        Sexp::Int(value.into())
    }

    /// The absent option, `()`. Identical to the empty sequence; see
    /// `req:textproj:schema-directed`.
    pub fn none() -> Sexp {
        Sexp::List(Vec::new())
    }

    /// The present option, `(some <value>)`.
    pub fn some(value: Sexp) -> Sexp {
        Sexp::List(vec![Sexp::sym("some"), value])
    }

    /// `(ratio <numerator> <denominator>)`.
    pub fn ratio(numerator: BigInt, denominator: BigInt) -> Sexp {
        Sexp::List(vec![
            Sexp::sym("ratio"),
            Sexp::Int(numerator),
            Sexp::Int(denominator),
        ])
    }

    /// The canonical rendering. Total, and injective by construction: every
    /// choice a writer could make is made here, once.
    pub fn render(&self) -> String {
        let mut out = String::new();
        self.write(&mut out);
        out
    }

    /// Appends the canonical rendering to `out`.
    pub fn write(&self, out: &mut String) {
        match self {
            Sexp::List(items) => {
                out.push('(');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(' ');
                    }
                    item.write(out);
                }
                out.push(')');
            }
            Sexp::Symbol(s) => out.push_str(s),
            Sexp::Int(n) => out.push_str(&n.to_string()),
            Sexp::Bytes(b) => {
                out.push_str("#x");
                for byte in b {
                    out.push(HEX[(byte >> 4) as usize]);
                    out.push(HEX[(byte & 0x0f) as usize]);
                }
            }
            Sexp::Str(s) => {
                out.push('"');
                for c in s.chars() {
                    match c {
                        '"' => out.push_str("\\\""),
                        '\\' => out.push_str("\\\\"),
                        '\n' => out.push_str("\\n"),
                        '\t' => out.push_str("\\t"),
                        _ => out.push(c),
                    }
                }
                out.push('"');
            }
        }
    }

    /// The list's elements, or `None` if this is not a list.
    pub fn as_list(&self) -> Option<&[Sexp]> {
        match self {
            Sexp::List(items) => Some(items),
            _ => None,
        }
    }

    /// The symbol's name, or `None` if this is not a symbol.
    pub fn as_symbol(&self) -> Option<&str> {
        match self {
            Sexp::Symbol(s) => Some(s),
            _ => None,
        }
    }

    /// A short description of this node's lexical class, for error messages.
    fn class(&self) -> &'static str {
        match self {
            Sexp::List(_) => "list",
            Sexp::Symbol(_) => "symbol",
            Sexp::Int(_) => "integer",
            Sexp::Bytes(_) => "byte string",
            Sexp::Str(_) => "string",
        }
    }
}

const HEX: [char; 16] = [
    '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', 'a', 'b', 'c', 'd', 'e', 'f',
];

// ===========================================================================
// The strict reader.
// ===========================================================================

/// Reads exactly one s-expression from `line`, which must be the whole of `line`.
///
/// Strict in the sense of `req:textproj:strict-parse`: it rejects every spelling
/// that is not the one [`Sexp::write`] produces. There is no normalization here
/// and no leniency to be exploited — a leading zero, an upper-case hex digit, a
/// doubled space, a space after `(`, an unknown escape and a trailing character
/// are all rejections.
///
/// It does **not** know types, so it cannot reject a value that is well-formed but
/// wrong for its position; that is [`TextValue::parse`]'s job. Together the two
/// give the layer-1 guarantee described in this module's header.
pub fn read_sexp(line: &str) -> Result<Sexp, TextError> {
    let mut reader = Reader {
        text: line,
        bytes: line.as_bytes(),
        at: 0,
    };
    let value = reader.value()?;
    if reader.at != line.len() {
        return Err(TextError::Syntax("trailing input after the s-expression"));
    }
    Ok(value)
}

struct Reader<'a> {
    text: &'a str,
    bytes: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }

    fn value(&mut self) -> Result<Sexp, TextError> {
        match self.peek() {
            Some(b'(') => self.list(),
            Some(b'"') => self.string(),
            Some(b'#') => self.byte_string(),
            Some(c) if c == b'-' || c.is_ascii_digit() => self.integer(),
            Some(c) if c.is_ascii_lowercase() => self.symbol(),
            Some(_) => Err(TextError::Syntax("no value begins with this character")),
            None => Err(TextError::Syntax("unexpected end of input")),
        }
    }

    /// `"(" (value (" " value)*)? ")"`. Exactly one space between elements, none
    /// after `(` and none before `)`.
    fn list(&mut self) -> Result<Sexp, TextError> {
        self.at += 1; // '('
        let mut items = Vec::new();
        if self.peek() == Some(b')') {
            self.at += 1;
            return Ok(Sexp::List(items));
        }
        loop {
            // Diagnostic only, as above: a space here is already rejected by
            // `value`, which knows no value beginning with U+0020. Kept so the
            // message says what is wrong, and tested so it keeps saying it.
            if self.peek() == Some(b' ') {
                return Err(TextError::Syntax(
                    "a list element may not be preceded by a space",
                ));
            }
            items.push(self.value()?);
            match self.peek() {
                Some(b' ') => self.at += 1,
                Some(b')') => {
                    self.at += 1;
                    return Ok(Sexp::List(items));
                }
                Some(_) => return Err(TextError::Syntax("expected a space or `)`")),
                None => return Err(TextError::Syntax("unclosed list")),
            }
        }
    }

    fn symbol(&mut self) -> Result<Sexp, TextError> {
        let start = self.at;
        // `symbol ::= [a-z] [a-z0-9-]*`
        self.at += 1;
        while let Some(c) = self.peek() {
            if c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-' {
                self.at += 1;
            } else {
                break;
            }
        }
        Ok(Sexp::Symbol(
            std::str::from_utf8(&self.bytes[start..self.at])
                .expect("ascii")
                .to_owned(),
        ))
    }

    /// `integer ::= "-"? digit+`, with no leading zeros and no `-0`.
    fn integer(&mut self) -> Result<Sexp, TextError> {
        let start = self.at;
        if self.peek() == Some(b'-') {
            self.at += 1;
        }
        let digits_start = self.at;
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.at += 1;
        }
        let digits = &self.bytes[digits_start..self.at];
        if digits.is_empty() {
            return Err(TextError::Syntax("an integer needs at least one digit"));
        }
        if digits.len() > 1 && digits[0] == b'0' {
            return Err(TextError::NotCanonical("integer has a leading zero"));
        }
        if digits == b"0" && start != digits_start {
            return Err(TextError::NotCanonical("negative zero"));
        }
        let text = std::str::from_utf8(&self.bytes[start..self.at]).expect("ascii");
        text.parse::<BigInt>()
            .map(Sexp::Int)
            .map_err(|_| TextError::Syntax("malformed integer"))
    }

    /// `bytes ::= "#x" hexdigit*`, an even number of lower-case hex digits.
    fn byte_string(&mut self) -> Result<Sexp, TextError> {
        self.at += 1; // '#'
        if self.peek() != Some(b'x') {
            return Err(TextError::Syntax("a byte string begins `#x`"));
        }
        self.at += 1;
        let start = self.at;
        while self
            .peek()
            .is_some_and(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        {
            self.at += 1;
        }
        // Diagnostic only, and known to be so: an upper-case hex digit merely stops
        // the scan, so it is *already* rejected downstream — by the odd-digit count
        // (`#x0A`) or by the trailing-input check (`#xAA`). Deleting this branch
        // changes no verdict. It is here to name the real problem, and
        // `the_reader_names_the_real_problem` holds it to that.
        if self.peek().is_some_and(|c| c.is_ascii_uppercase()) {
            return Err(TextError::NotCanonical("byte string uses upper-case hex"));
        }
        let digits = &self.bytes[start..self.at];
        if digits.len() % 2 != 0 {
            return Err(TextError::Syntax("byte string has an odd hex digit count"));
        }
        let mut out = Vec::with_capacity(digits.len() / 2);
        for pair in digits.chunks_exact(2) {
            let hi = (pair[0] as char).to_digit(16).expect("checked") as u8;
            let lo = (pair[1] as char).to_digit(16).expect("checked") as u8;
            out.push((hi << 4) | lo);
        }
        Ok(Sexp::Bytes(out))
    }

    /// `string ::= '"' schar* '"'`, where `schar` is an unescaped scalar other
    /// than U+0022, U+005C, U+000A, U+0009, or one of exactly four escapes.
    fn string(&mut self) -> Result<Sexp, TextError> {
        self.at += 1; // '"'
        let mut out = String::new();
        loop {
            let rest = &self.bytes[self.at..];
            let Some(&first) = rest.first() else {
                return Err(TextError::Syntax("unterminated string"));
            };
            match first {
                b'"' => {
                    self.at += 1;
                    return Ok(Sexp::Str(out));
                }
                b'\\' => {
                    let Some(&escaped) = rest.get(1) else {
                        return Err(TextError::Syntax("unterminated escape"));
                    };
                    out.push(match escaped {
                        b'"' => '"',
                        b'\\' => '\\',
                        b'n' => '\n',
                        b't' => '\t',
                        _ => return Err(TextError::NotCanonical("unknown escape sequence")),
                    });
                    self.at += 2;
                }
                b'\n' | b'\t' => {
                    return Err(TextError::NotCanonical(
                        "U+000A and U+0009 must be escaped in a string",
                    ))
                }
                _ => {
                    // One whole UTF-8 scalar. `self.at` is always a char boundary:
                    // every advance above is by an ASCII byte or by `len_utf8`.
                    let c = self.text[self.at..].chars().next().expect("non-empty");
                    out.push(c);
                    self.at += c.len_utf8();
                }
            }
        }
    }
}

// ===========================================================================
// Errors.
// ===========================================================================

/// Why a projection could not be read.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum TextError {
    /// A node of the wrong lexical class: expected `expected`, found `found`.
    Expected {
        /// What the schema expected at this position.
        expected: &'static str,
        /// The lexical class actually present.
        found: &'static str,
    },
    /// A list of the wrong length for the type expected at this position.
    Arity {
        /// The type expected.
        type_name: &'static str,
        /// How many elements the type has.
        expected: usize,
        /// How many were present.
        found: usize,
    },
    /// A constructor symbol that names no variant of the expected type.
    UnknownConstructor {
        /// The type expected.
        type_name: &'static str,
        /// The constructor found.
        found: String,
    },
    /// A leaf whose text is not the canonical spelling of any value: a
    /// non-reduced rational, an integer with a leading zero, an out-of-range
    /// value, a non-NFC catalog id.
    NotCanonical(&'static str),
    /// A set or map whose elements are not strictly increasing, or a sequence the
    /// binary form constrains and this text violates. Never normalized.
    NotStrictlyIncreasing(&'static str),
    /// The text is not well-formed: bad escape, odd hex digit count, stray
    /// whitespace, unbalanced parenthesis, trailing input.
    Syntax(&'static str),
}

impl fmt::Display for TextError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TextError::Expected { expected, found } => {
                write!(f, "expected {expected}, found {found}")
            }
            TextError::Arity {
                type_name,
                expected,
                found,
            } => write!(f, "{type_name} has {expected} fields, found {found}"),
            TextError::UnknownConstructor { type_name, found } => {
                write!(f, "`{found}` names no variant of {type_name}")
            }
            TextError::NotCanonical(what) => write!(f, "not canonical: {what}"),
            TextError::NotStrictlyIncreasing(what) => {
                write!(f, "not strictly increasing: {what}")
            }
            TextError::Syntax(what) => write!(f, "syntax: {what}"),
        }
    }
}

impl std::error::Error for TextError {}

impl Sexp {
    /// Asserts this node is a list of exactly `arity` elements headed by the
    /// symbol `name`, and returns the fields after the head.
    ///
    /// A struct with zero fields never reaches here: it is the bare symbol
    /// `name`, as a fieldless variant is.
    pub fn expect_struct(&self, name: &str, arity: usize) -> Result<&[Sexp], TextError> {
        let items = self.as_list().ok_or(TextError::Expected {
            expected: "struct",
            found: self.class(),
        })?;
        match items.first().and_then(Sexp::as_symbol) {
            Some(head) if head == name => {}
            Some(head) => {
                return Err(TextError::UnknownConstructor {
                    type_name: "struct",
                    found: head.to_owned(),
                })
            }
            None => return Err(TextError::Syntax("a struct is headed by its type name")),
        }
        if items.len() != arity + 1 {
            return Err(TextError::Arity {
                type_name: "struct",
                expected: arity,
                found: items.len().saturating_sub(1),
            });
        }
        Ok(&items[1..])
    }
}

// ===========================================================================
// Names.
// ===========================================================================

/// A Rust type or variant name in the projection's lower-case hyphenated form:
/// `PitchedEvent` becomes `pitched-event`, `EventOrderingDAG` becomes
/// `event-ordering-dag`, `CanonicalF64` becomes `canonical-f64`.
///
/// A hyphen is inserted before an upper-case letter that follows a lower-case
/// letter or a digit, and before the last upper-case letter of a run when a
/// lower-case letter follows it (so `DAGNode` becomes `dag-node`).
pub fn kebab(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    let mut out = String::with_capacity(name.len() + 4);
    for (i, &c) in chars.iter().enumerate() {
        if c.is_ascii_uppercase() && i > 0 {
            let prev = chars[i - 1];
            let next_is_lower = chars.get(i + 1).is_some_and(|n| n.is_ascii_lowercase());
            if prev.is_ascii_lowercase()
                || prev.is_ascii_digit()
                || (prev.is_ascii_uppercase() && next_is_lower)
            {
                out.push('-');
            }
        }
        out.push(c.to_ascii_lowercase());
    }
    out
}

// ===========================================================================
// The trait.
// ===========================================================================

/// A canonical value that projects to text and parses back.
///
/// `parse` **must not normalize** (`req:textproj:strict-parse`). Where an impl
/// cannot avoid constructing through a normalizing constructor, it constructs and
/// then compares against its input, rejecting on difference.
pub trait TextValue: Sized {
    /// This value's canonical projection.
    fn project(&self) -> Sexp;

    /// Reads a value of this type from its projection, rejecting any text that is
    /// not the canonical projection of the value it denotes.
    fn parse(s: &Sexp) -> Result<Self, TextError>;
}

// ===========================================================================
// Leaf impls.
// ===========================================================================

impl TextValue for bool {
    fn project(&self) -> Sexp {
        Sexp::sym(if *self { "true" } else { "false" })
    }
    fn parse(s: &Sexp) -> Result<Self, TextError> {
        match s.as_symbol() {
            Some("true") => Ok(true),
            Some("false") => Ok(false),
            Some(_) => Err(TextError::NotCanonical("a boolean is `true` or `false`")),
            None => Err(TextError::Expected {
                expected: "boolean",
                found: s.class(),
            }),
        }
    }
}

impl TextValue for String {
    fn project(&self) -> Sexp {
        Sexp::Str(self.clone())
    }
    fn parse(s: &Sexp) -> Result<Self, TextError> {
        match s {
            Sexp::Str(v) => Ok(v.clone()),
            _ => Err(TextError::Expected {
                expected: "string",
                found: s.class(),
            }),
        }
    }
}

/// Integers project as integers, and parse back only within their own range. The
/// text carries no width, so the expected type supplies it — schema-directed
/// again.
macro_rules! int_text_value {
    ($($ty:ty),* $(,)?) => {
        $(
            impl TextValue for $ty {
                fn project(&self) -> Sexp {
                    Sexp::Int(BigInt::from(*self))
                }
                fn parse(s: &Sexp) -> Result<Self, TextError> {
                    match s {
                        Sexp::Int(n) => <$ty>::try_from(n.clone()).map_err(|_| {
                            TextError::NotCanonical(concat!(
                                "integer out of range for ",
                                stringify!($ty)
                            ))
                        }),
                        _ => Err(TextError::Expected {
                            expected: "integer",
                            found: s.class(),
                        }),
                    }
                }
            }
        )*
    };
}

int_text_value!(u8, u16, u32, u64, u128, i8, i16, i32, i64);

// ===========================================================================
// Collection impls.
// ===========================================================================

impl<T: TextValue> TextValue for Option<T> {
    fn project(&self) -> Sexp {
        match self {
            None => Sexp::none(),
            Some(v) => Sexp::some(v.project()),
        }
    }
    fn parse(s: &Sexp) -> Result<Self, TextError> {
        let items = s.as_list().ok_or(TextError::Expected {
            expected: "option",
            found: s.class(),
        })?;
        match items {
            [] => Ok(None),
            [head, value] if head.as_symbol() == Some("some") => Ok(Some(T::parse(value)?)),
            _ => Err(TextError::Syntax("an option is `()` or `(some <value>)`")),
        }
    }
}

impl<T: TextValue> TextValue for Vec<T> {
    fn project(&self) -> Sexp {
        Sexp::List(self.iter().map(T::project).collect())
    }
    fn parse(s: &Sexp) -> Result<Self, TextError> {
        let items = s.as_list().ok_or(TextError::Expected {
            expected: "sequence",
            found: s.class(),
        })?;
        items.iter().map(T::parse).collect()
    }
}

/// A set projects strictly increasing and parses strictly increasing.
///
/// Absorbing a duplicate into the set, or letting `BTreeSet::insert` silently
/// re-sort, would be **normalizing** — the thing `req:textproj:strict-parse`
/// forbids. The check is per-element and explicit.
impl<T: TextValue + Ord> TextValue for BTreeSet<T> {
    fn project(&self) -> Sexp {
        Sexp::List(self.iter().map(T::project).collect())
    }
    fn parse(s: &Sexp) -> Result<Self, TextError> {
        let items = s.as_list().ok_or(TextError::Expected {
            expected: "set",
            found: s.class(),
        })?;
        let mut out = BTreeSet::new();
        for item in items {
            let value = T::parse(item)?;
            // Only ever-increasing values are inserted, so the set's maximum is
            // the previously read element.
            if out.last().is_some_and(|previous| *previous >= value) {
                return Err(TextError::NotStrictlyIncreasing("set elements"));
            }
            out.insert(value);
        }
        Ok(out)
    }
}

/// A pair projects as a two-element list, exactly as a map entry does — the
/// binary form writes its two components in order and adds nothing, so neither
/// does the text.
impl<A: TextValue, B: TextValue> TextValue for (A, B) {
    fn project(&self) -> Sexp {
        Sexp::List(vec![self.0.project(), self.1.project()])
    }
    fn parse(s: &Sexp) -> Result<Self, TextError> {
        let items = s.as_list().ok_or(TextError::Expected {
            expected: "pair",
            found: s.class(),
        })?;
        let [first, second] = items else {
            return Err(TextError::Syntax("a pair is `(<first> <second>)`"));
        };
        Ok((A::parse(first)?, B::parse(second)?))
    }
}

/// A map projects as a list of `(<key> <value>)` entries, strictly increasing by
/// key, and parses the same. Same reasoning as [`BTreeSet`].
impl<K: TextValue + Ord, V: TextValue> TextValue for BTreeMap<K, V> {
    fn project(&self) -> Sexp {
        Sexp::List(
            self.iter()
                .map(|(k, v)| Sexp::List(vec![k.project(), v.project()]))
                .collect(),
        )
    }
    fn parse(s: &Sexp) -> Result<Self, TextError> {
        let items = s.as_list().ok_or(TextError::Expected {
            expected: "map",
            found: s.class(),
        })?;
        let mut out = BTreeMap::new();
        for item in items {
            let entry = item.as_list().ok_or(TextError::Expected {
                expected: "map entry",
                found: item.class(),
            })?;
            let [key, value] = entry else {
                return Err(TextError::Syntax("a map entry is `(<key> <value>)`"));
            };
            let key = K::parse(key)?;
            // As for a set: keys only increase, so the map's maximum key is the
            // previously read one.
            if out
                .last_key_value()
                .is_some_and(|(previous, _)| *previous >= key)
            {
                return Err(TextError::NotStrictlyIncreasing("map keys"));
            }
            let value = V::parse(value)?;
            out.insert(key, value);
        }
        Ok(out)
    }
}

// ===========================================================================
// Big integers.
// ===========================================================================

impl TextValue for BigInt {
    fn project(&self) -> Sexp {
        Sexp::Int(self.clone())
    }
    fn parse(s: &Sexp) -> Result<Self, TextError> {
        match s {
            Sexp::Int(n) => Ok(n.clone()),
            _ => Err(TextError::Expected {
                expected: "integer",
                found: s.class(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kebab_handles_acronyms_digits_and_runs() {
        assert_eq!(kebab("PitchedEvent"), "pitched-event");
        assert_eq!(kebab("Pitch"), "pitch");
        assert_eq!(kebab("EventOrderingDAG"), "event-ordering-dag");
        assert_eq!(kebab("CanonicalF64"), "canonical-f64");
        assert_eq!(kebab("DAGNode"), "dag-node");
        assert_eq!(kebab("TimeSignatureDisplay"), "time-signature-display");
        assert_eq!(kebab("ArticulationMark"), "articulation-mark");
    }

    #[test]
    fn rendering_is_canonical() {
        assert_eq!(Sexp::Bytes(vec![0x0a, 0xff]).render(), "#x0aff");
        assert_eq!(Sexp::Bytes(vec![]).render(), "#x");
        assert_eq!(Sexp::none().render(), "()");
        assert_eq!(Sexp::some(Sexp::sym("x")).render(), "(some x)");
        assert_eq!(true.project().render(), "true");
        assert_eq!(Sexp::int(-5i32).render(), "-5");
        assert_eq!(Sexp::int(0i32).render(), "0");
    }

    #[test]
    fn strings_escape_exactly_the_four_characters() {
        let s = "a\"b\\c\nd\te\u{00e9}".to_string();
        assert_eq!(s.project().render(), "\"a\\\"b\\\\c\\nd\\te\u{00e9}\"");
    }

    /// Whatever `write` emits, `read_sexp` must accept and return unchanged.
    #[track_caller]
    fn round_trips(text: &str) {
        let value = read_sexp(text).unwrap_or_else(|e| panic!("{text:?} rejected: {e}"));
        assert_eq!(value.render(), text, "re-rendering {text:?} changed it");
    }

    /// `read_sexp` must reject; the message is not part of the contract.
    #[track_caller]
    fn rejected(text: &str) {
        assert!(
            read_sexp(text).is_err(),
            "{text:?} was accepted; strict parsing forbids it"
        );
    }

    #[test]
    fn the_reader_accepts_exactly_what_the_writer_emits() {
        for text in [
            "()",
            "(some x)",
            "#x",
            "#x0aff",
            "0",
            "-5",
            "12345678901234567890123456789",
            "true",
            "false",
            "(ratio -3 4)",
            "\"\"",
            "\"a\\\"b\\\\c\\nd\\te\"",
            "(pitched-event #x0a (articulation-mark) () stem-configuration)",
            "((a b) (c d))",
        ] {
            round_trips(text);
        }
    }

    #[test]
    fn the_reader_rejects_every_non_canonical_spelling() {
        // Whitespace has nowhere to hide (`req:textproj:envelope-per-line`).
        rejected("( a)");
        rejected("(a )");
        rejected("(a  b)");
        rejected("(a\tb)");
        rejected(" ()");
        rejected("() ");

        // Integers: no leading zeros, no negative zero, no `+`.
        rejected("007");
        rejected("-0");
        rejected("-01");
        rejected("+1");
        rejected("-");

        // Byte strings: lower-case hex, even count, `#x` prefix.
        rejected("#x0A");
        rejected("#xAA");
        rejected("#x0");
        rejected("#y00");
        rejected("#");

        // Strings: exactly four escapes; the four characters never appear raw.
        rejected("\"\\q\"");
        rejected("\"\\\"");
        rejected("\"a");
        rejected("\"a\tb\"");
        rejected("\"a\nb\"");

        // Structure.
        rejected("(");
        rejected(")");
        rejected("(a");
        rejected("(a))");
        rejected("(some(x))");
        rejected("");
        rejected("A");
        rejected("-a");
    }

    /// Two branches of the reader change no verdict — an upper-case hex digit and
    /// a stray space are rejected downstream anyway. Mutation testing found both
    /// surviving the rejection suite above, which proves only that *something*
    /// rejects them. Their real contract is the diagnostic, so that is what is
    /// asserted here; without this test, deleting either branch is invisible.
    #[test]
    fn the_reader_names_the_real_problem() {
        assert_eq!(
            read_sexp("#xAA"),
            Err(TextError::NotCanonical("byte string uses upper-case hex"))
        );
        assert_eq!(
            read_sexp("#x0A"),
            Err(TextError::NotCanonical("byte string uses upper-case hex"))
        );
        assert_eq!(
            read_sexp("( a)"),
            Err(TextError::Syntax(
                "a list element may not be preceded by a space"
            ))
        );
        assert_eq!(
            read_sexp("(a  b)"),
            Err(TextError::Syntax(
                "a list element may not be preceded by a space"
            ))
        );
    }

    #[test]
    fn a_set_parses_only_strictly_increasing_elements() {
        let ok = read_sexp("(1 2 3)").unwrap();
        assert_eq!(
            BTreeSet::<u8>::parse(&ok).unwrap(),
            BTreeSet::from([1, 2, 3])
        );

        // Absorbing either of these into the set would be normalizing.
        for bad in ["(1 1 2)", "(3 2 1)", "(1 3 2)"] {
            let s = read_sexp(bad).unwrap();
            assert_eq!(
                BTreeSet::<u8>::parse(&s),
                Err(TextError::NotStrictlyIncreasing("set elements")),
                "{bad} must be rejected, not sorted"
            );
        }
    }

    #[test]
    fn a_map_parses_only_strictly_increasing_keys() {
        let ok = read_sexp("((1 true) (2 false))").unwrap();
        assert_eq!(
            BTreeMap::<u8, bool>::parse(&ok).unwrap(),
            BTreeMap::from([(1, true), (2, false)])
        );
        for bad in ["((2 true) (1 false))", "((1 true) (1 false))"] {
            let s = read_sexp(bad).unwrap();
            assert_eq!(
                BTreeMap::<u8, bool>::parse(&s),
                Err(TextError::NotStrictlyIncreasing("map keys")),
                "{bad} must be rejected"
            );
        }
        let malformed = read_sexp("((1))").unwrap();
        assert!(BTreeMap::<u8, bool>::parse(&malformed).is_err());
    }

    /// `()` is the empty sequence *and* the absent option; only the expected type
    /// tells them apart. That is `req:textproj:schema-directed`, and here it is.
    #[test]
    fn the_empty_list_is_both_an_empty_sequence_and_an_absent_option() {
        let empty = read_sexp("()").unwrap();
        assert_eq!(Option::<u8>::parse(&empty).unwrap(), None);
        assert_eq!(Vec::<u8>::parse(&empty).unwrap(), Vec::<u8>::new());
        assert_eq!(BTreeSet::<u8>::parse(&empty).unwrap(), BTreeSet::new());
        assert_eq!(None::<u8>.project(), Vec::<u8>::new().project());
    }

    #[test]
    fn an_integer_parses_only_within_the_expected_type_s_range() {
        let big = read_sexp("256").unwrap();
        assert!(u8::parse(&big).is_err());
        assert_eq!(u16::parse(&big).unwrap(), 256);
        let negative = read_sexp("-1").unwrap();
        assert!(u8::parse(&negative).is_err());
        assert_eq!(i8::parse(&negative).unwrap(), -1);
    }
}
