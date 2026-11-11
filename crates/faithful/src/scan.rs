//! One pass over a CBOR body: well-formed, valid, and inside the JSON data
//! model, or the first reason it is not.
//!
//! Iterative, with an explicit stack bounded by [`MAX_DEPTH`], so no input
//! can overflow the thread's stack; a declared length is checked against the
//! bytes left before anything is read, so no input makes it allocate the
//! declared size (it allocates only to compare an indefinite-length map key).

use std::borrow::Cow;
use std::fmt;

/// The deepest nesting accepted: serde_json's limit, which `Json<T>` holds.
pub const MAX_DEPTH: usize = 128;

/// Why a body is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Rule {
    /// The input ended inside an item. 400.
    Truncated,
    /// Additional information 28, 29 or 30. 400.
    Reserved,
    /// A break code outside an indefinite-length item. 400.
    StrayBreak,
    /// Indefinite length on an integer or a tag. 400.
    IndefiniteArgument,
    /// An indefinite-length string holding something other than a
    /// definite-length string of its own type. 400.
    BadChunk,
    /// A two-byte simple value below 32. 400.
    SimpleTooShort,
    /// Bytes after the item. 400.
    Trailing,
    /// A text string that is not UTF-8. 400.
    InvalidUtf8,
    /// Nested deeper than [`MAX_DEPTH`]. 400.
    TooDeep,
    /// A byte string. 422.
    ByteString,
    /// A tag. 422.
    Tag,
    /// `undefined` or a simple value other than `false`, `true`, `null`. 422.
    Simple,
    /// NaN or an infinity. 422.
    NonFinite,
    /// A map key that is not a text string. 422.
    NonTextKey,
    /// A key repeated in one map. 422.
    DuplicateKey,
    /// Not preferred serialization (only [`preferred`] reports this).
    NotPreferred,
}

impl Rule {
    /// Whether the input is not a valid CBOR item at all (400), rather than
    /// a valid item the JSON data model has no place for (422).
    #[must_use]
    pub fn is_syntax(self) -> bool {
        matches!(
            self,
            Self::Truncated
                | Self::Reserved
                | Self::StrayBreak
                | Self::IndefiniteArgument
                | Self::BadChunk
                | Self::SimpleTooShort
                | Self::Trailing
                | Self::InvalidUtf8
                | Self::TooDeep
        )
    }

    /// What the rule says, without echoing input.
    #[must_use]
    pub fn describe(self) -> &'static str {
        match self {
            Self::Truncated => "the body ends inside a CBOR item",
            Self::Reserved => "a CBOR head uses a reserved additional-information value",
            Self::StrayBreak => "a CBOR break code appears outside an indefinite-length item",
            Self::IndefiniteArgument => "an integer or tag is given an indefinite length",
            Self::BadChunk => "an indefinite-length string holds a chunk of another type",
            Self::SimpleTooShort => "a two-byte simple value is below 32",
            Self::Trailing => "bytes follow the CBOR item",
            Self::InvalidUtf8 => "a CBOR text string is not UTF-8",
            Self::TooDeep => "the body is nested too deeply",
            Self::ByteString => "a byte string has no JSON equivalent the schema could describe",
            Self::Tag => "a CBOR tag has no JSON equivalent the schema could describe",
            Self::Simple => "a simple value other than false, true or null has no JSON equivalent",
            Self::NonFinite => "NaN and infinities are not JSON numbers",
            Self::NonTextKey => "a map key that is not a text string has no JSON equivalent",
            Self::DuplicateKey => "a map repeats a key",
            Self::NotPreferred => "not preferred serialization",
        }
    }
}

/// A refused body: the rule and the offset of the item it applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Refusal {
    /// Which rule.
    pub rule: Rule,
    /// Byte offset of the offending head.
    pub offset: usize,
}

impl Refusal {
    /// The status a server answers with.
    #[must_use]
    pub fn status(&self) -> u16 {
        if self.rule.is_syntax() { 400 } else { 422 }
    }
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (at byte {})", self.rule.describe(), self.offset)
    }
}

impl std::error::Error for Refusal {}

/// Check `body` against the rules (crate docs), in one pass.
///
/// # Errors
/// The first rule the body breaks, in input order.
pub fn scan(body: &[u8]) -> Result<(), Refusal> {
    Walker::new(body, Mode::Profile).run()
}

/// Whether `body` is one well-formed item in preferred serialization (RFC
/// 8949 §4.1): every argument in its shortest form, every float in the
/// shortest width that holds it exactly, every length definite. Byte strings
/// and tags are allowed here: this is a check of encoding, not of the data
/// model.
#[must_use]
pub fn preferred(body: &[u8]) -> bool {
    Walker::new(body, Mode::Preferred).run().is_ok()
}

/// Whether `body` is one well-formed CBOR item (RFC 8949 section 5.1) with
/// valid UTF-8 text, nothing else checked: what a generic decoder must
/// accept, and the line the fuzzer draws between an input a candidate must
/// refuse and one it may decode. Nesting is unbounded (the walk is
/// iterative).
///
/// # Errors
/// The first rule the body breaks.
pub fn well_formed(body: &[u8]) -> Result<(), Refusal> {
    Walker::new(body, Mode::WellFormed).run()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// The JSON-data-model profile.
    Profile,
    /// Well-formedness and UTF-8 alone.
    WellFormed,
    /// Well-formedness plus preferred serialization.
    Preferred,
}

/// What an open container still expects.
#[derive(Clone, Copy)]
enum Frame {
    /// A definite array or map with this many items left (a map counts
    /// keys and values), and, for a map, where its keys start in `keys`.
    Definite { left: u64, map: Option<usize> },
    /// An indefinite array, or map (with its keys' start), until a break.
    Indefinite { map: Option<usize>, items: u64 },
    /// An indefinite string of this major type, until a break.
    Chunks { major: u8 },
}

struct Walker<'a> {
    body: &'a [u8],
    at: usize,
    mode: Mode,
    stack: Vec<Frame>,
    /// Keys of every open map, innermost last.
    keys: Vec<Cow<'a, [u8]>>,
}

/// One decoded head.
struct Head {
    major: u8,
    info: u8,
    arg: u64,
    offset: usize,
}

impl<'a> Walker<'a> {
    fn new(body: &'a [u8], mode: Mode) -> Self {
        Self {
            body,
            at: 0,
            mode,
            stack: Vec::new(),
            keys: Vec::new(),
        }
    }

    fn take(&mut self, n: u64, offset: usize) -> Result<&'a [u8], Refusal> {
        let left = (self.body.len() - self.at) as u64;
        if n > left {
            return refuse(Rule::Truncated, offset);
        }
        let start = self.at;
        self.at += n as usize;
        Ok(&self.body[start..self.at])
    }

    fn head(&mut self) -> Result<Head, Refusal> {
        let offset = self.at;
        let Some(&first) = self.body.get(self.at) else {
            return refuse(Rule::Truncated, offset);
        };
        self.at += 1;
        let (major, info) = (first >> 5, first & 0x1f);
        let arg = match info {
            0..=23 => u64::from(info),
            24..=27 => {
                let width = 1u64 << (info - 24);
                let bytes = self.take(width, offset)?;
                let arg = bytes.iter().fold(0u64, |acc, b| acc << 8 | u64::from(*b));
                // Floats (major 7, 25..=27) are checked for width separately.
                if self.mode == Mode::Preferred && major != 7 && shorter_head_fits(info, arg) {
                    return refuse(Rule::NotPreferred, offset);
                }
                arg
            }
            28..=30 => return refuse(Rule::Reserved, offset),
            _ => 0,
        };
        Ok(Head {
            major,
            info,
            arg,
            offset,
        })
    }

    #[expect(clippy::too_many_lines, reason = "one state machine, read top to bottom")]
    fn run(mut self) -> Result<(), Refusal> {
        loop {
            let head = self.head()?;
            let is_key = self.expecting_key();
            let is_break = head.major == 7 && head.info == 31;
            if let Some(Frame::Chunks { major }) = self.stack.last().copied() {
                if is_break {
                    self.stack.pop();
                    if self.item_done(head.offset)? {
                        return self.finish();
                    }
                    continue;
                }
                if head.major != major || head.info == 31 {
                    return refuse(Rule::BadChunk, head.offset);
                }
                let chunk = self.take(head.arg, head.offset)?;
                if major == 3 && std::str::from_utf8(chunk).is_err() {
                    return refuse(Rule::InvalidUtf8, head.offset);
                }
                continue;
            }
            if is_break {
                match self.stack.last().copied() {
                    Some(Frame::Indefinite { map, items }) => {
                        if map.is_some() && items % 2 == 1 {
                            // A key with no value.
                            return refuse(Rule::StrayBreak, head.offset);
                        }
                        self.stack.pop();
                        if let Some(start) = map {
                            self.close_map(start, head.offset)?;
                        }
                        if self.item_done(head.offset)? {
                            return self.finish();
                        }
                        continue;
                    }
                    _ => return refuse(Rule::StrayBreak, head.offset),
                }
            }
            if is_key && self.mode == Mode::Profile && head.major != 3 {
                return refuse(Rule::NonTextKey, head.offset);
            }
            if head.info == 31 {
                if self.mode == Mode::Preferred {
                    return refuse(Rule::NotPreferred, head.offset);
                }
                match head.major {
                    2 | 3 => {
                        if head.major == 2 && self.mode == Mode::Profile {
                            return refuse(Rule::ByteString, head.offset);
                        }
                        if is_key && head.major == 3 {
                            // An indefinite key is compared by its
                            // concatenation, which needs a copy.
                            let key = self.indefinite_key(head.offset)?;
                            self.keys.push(Cow::Owned(key));
                            if self.item_done(head.offset)? {
                                return self.finish();
                            }
                        } else {
                            self.stack.push(Frame::Chunks { major: head.major });
                        }
                        continue;
                    }
                    4 | 5 => {
                        self.open(
                            Frame::Indefinite {
                                map: (head.major == 5).then_some(self.keys.len()),
                                items: 0,
                            },
                            head.offset,
                        )?;
                        continue;
                    }
                    _ => return refuse(Rule::IndefiniteArgument, head.offset),
                }
            }
            match head.major {
                0 | 1 => {}
                2 => {
                    if self.mode == Mode::Profile {
                        return refuse(Rule::ByteString, head.offset);
                    }
                    self.take(head.arg, head.offset)?;
                }
                3 => {
                    let text = self.take(head.arg, head.offset)?;
                    if std::str::from_utf8(text).is_err() {
                        return refuse(Rule::InvalidUtf8, head.offset);
                    }
                    if is_key {
                        self.keys.push(Cow::Borrowed(text));
                    }
                }
                4 | 5 => {
                    let items = if head.major == 5 {
                        head.arg.checked_mul(2)
                    } else {
                        Some(head.arg)
                    };
                    // Every item takes at least a byte: a longer count than
                    // the bytes left cannot be met, whatever it allocates.
                    match items {
                        Some(n) if n <= (self.body.len() - self.at) as u64 => {}
                        _ => return refuse(Rule::Truncated, head.offset),
                    }
                    let map = (head.major == 5).then_some(self.keys.len());
                    if items == Some(0) {
                        if let Some(start) = map {
                            self.close_map(start, head.offset)?;
                        }
                    } else {
                        self.open(
                            Frame::Definite {
                                left: items.unwrap_or(0),
                                map,
                            },
                            head.offset,
                        )?;
                        continue;
                    }
                }
                6 => {
                    if self.mode == Mode::Profile {
                        return refuse(Rule::Tag, head.offset);
                    }
                    // A tag is followed by its content, which is the item.
                    continue;
                }
                _ => self.simple_or_float(&head)?,
            }
            if self.item_done(head.offset)? {
                return self.finish();
            }
        }
    }

    fn simple_or_float(&mut self, head: &Head) -> Result<(), Refusal> {
        match head.info {
            24 if head.arg < 32 => refuse(Rule::SimpleTooShort, head.offset),
            0..=19 | 23 | 24 if self.mode == Mode::Profile => refuse(Rule::Simple, head.offset),
            0..=24 => Ok(()),
            25..=27 => {
                let value = match head.info {
                    25 => f16_to_f64(head.arg as u16),
                    26 => f64::from(f32::from_bits(head.arg as u32)),
                    _ => f64::from_bits(head.arg),
                };
                if self.mode == Mode::Profile && !value.is_finite() {
                    return refuse(Rule::NonFinite, head.offset);
                }
                if self.mode == Mode::Preferred && !shortest_float(head.info, head.arg) {
                    return refuse(Rule::NotPreferred, head.offset);
                }
                Ok(())
            }
            _ => unreachable!("additional information 28..=31 is handled before"),
        }
    }

    /// Whether the next item is a map key.
    fn expecting_key(&self) -> bool {
        match self.stack.last() {
            Some(Frame::Definite { left, map: Some(_) }) => left % 2 == 0,
            Some(Frame::Indefinite { map: Some(_), items }) => items % 2 == 0,
            _ => false,
        }
    }

    fn open(&mut self, frame: Frame, offset: usize) -> Result<(), Refusal> {
        // The stack is on the heap, so only the profile needs a bound.
        if self.mode != Mode::Profile {
            self.stack.push(frame);
            return Ok(());
        }
        let depth = self.stack.iter().filter(|f| !matches!(f, Frame::Chunks { .. })).count();
        if depth >= MAX_DEPTH {
            return refuse(Rule::TooDeep, offset);
        }
        self.stack.push(frame);
        Ok(())
    }

    /// An item finished: count it against its container, closing every
    /// container it completes. True when the top-level item is done.
    fn item_done(&mut self, offset: usize) -> Result<bool, Refusal> {
        loop {
            match self.stack.last_mut() {
                None => return Ok(true),
                Some(Frame::Indefinite { items, .. }) => {
                    *items += 1;
                    return Ok(false);
                }
                Some(Frame::Chunks { .. }) => return Ok(false),
                Some(Frame::Definite { left, map }) => {
                    *left -= 1;
                    if *left > 0 {
                        return Ok(false);
                    }
                    let map = *map;
                    self.stack.pop();
                    if let Some(start) = map {
                        self.close_map(start, offset)?;
                    }
                }
            }
        }
    }

    /// Refuse a map whose keys (`keys[start..]`) repeat, then forget them.
    fn close_map(&mut self, start: usize, offset: usize) -> Result<(), Refusal> {
        if self.mode == Mode::Profile {
            let keys = &mut self.keys[start..];
            keys.sort_unstable();
            if keys.windows(2).any(|pair| pair[0] == pair[1]) {
                return refuse(Rule::DuplicateKey, offset);
            }
        }
        self.keys.truncate(start);
        Ok(())
    }

    /// Read an indefinite-length text key's chunks into one key.
    fn indefinite_key(&mut self, offset: usize) -> Result<Vec<u8>, Refusal> {
        let mut key = Vec::new();
        loop {
            let head = self.head()?;
            if head.major == 7 && head.info == 31 {
                if std::str::from_utf8(&key).is_err() {
                    return refuse(Rule::InvalidUtf8, offset);
                }
                return Ok(key);
            }
            if head.major != 3 || head.info == 31 {
                return refuse(Rule::BadChunk, head.offset);
            }
            key.extend_from_slice(self.take(head.arg, head.offset)?);
        }
    }

    fn finish(&self) -> Result<(), Refusal> {
        if self.at == self.body.len() {
            Ok(())
        } else {
            refuse(Rule::Trailing, self.at)
        }
    }
}

fn refuse<T>(rule: Rule, offset: usize) -> Result<T, Refusal> {
    Err(Refusal { rule, offset })
}

/// Whether an argument written with additional information `info` fits a
/// shorter head.
fn shorter_head_fits(info: u8, arg: u64) -> bool {
    match info {
        24 => arg < 24,
        25 => arg <= 0xff,
        26 => arg <= 0xffff,
        _ => arg <= 0xffff_ffff,
    }
}

/// An IEEE 754 half-precision value, widened exactly.
#[must_use]
fn f16_to_f64(bits: u16) -> f64 {
    let sign = if bits & 0x8000 == 0 { 1.0 } else { -1.0 };
    let exponent = i32::from((bits >> 10) & 0x1f);
    let mantissa = f64::from(bits & 0x3ff);
    sign * match exponent {
        0 => mantissa * 2f64.powi(-24),
        31 if mantissa == 0.0 => f64::INFINITY,
        31 => f64::NAN,
        _ => (1.0 + mantissa / 1024.0) * 2f64.powi(exponent - 15),
    }
}

/// Whether a float of `info`'s width is in the shortest width that holds it
/// exactly (NaN is shortest only as a half).
fn shortest_float(info: u8, bits: u64) -> bool {
    let value = match info {
        25 => return true,
        26 => f64::from(f32::from_bits(bits as u32)),
        _ => f64::from_bits(bits),
    };
    if value.is_nan() {
        return false;
    }
    let fits_half = half_exact(value);
    match info {
        26 => !fits_half,
        _ => !fits_half && f64::from(value as f32).to_bits() != value.to_bits(),
    }
}

/// Whether a half-precision float holds `value` exactly.
fn half_exact(value: f64) -> bool {
    if value.is_infinite() || value == 0.0 {
        return true;
    }
    let magnitude = value.abs();
    // Halves hold multiples of 2^-24 below 2^-14 (subnormals), and 11
    // significant bits from 2^-14 to 65504.
    if magnitude > 65504.0 {
        return false;
    }
    let exponent = magnitude.log2().floor() as i32;
    let quantum = 2f64.powi(exponent.max(-14) - 10);
    (magnitude / quantum).fract() == 0.0
}

#[cfg(test)]
mod tests;
