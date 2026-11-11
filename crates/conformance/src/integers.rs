//! Section `integers`: every 64-bit argument, at the boundaries of each
//! target type, and beyond 64 bits.
//!
//! Gates: an in-range integer decoded to another value or refused, and an
//! out-of-range one accepted (silently wrapped or narrowed) or refused with
//! 400 (it is a valid item of the wrong shape: 422). Recorded: integers
//! beyond 64 bits (bignums, tags 2 and 3) and `-2^64`, which #989 asks to be
//! listed as accepted or refused.

use codecs::Backend;
use serde_json::{Value, json};

use crate::cases::{decode, hex};
use crate::outcome::Outcome;
use crate::record::Section;

/// `(what, bytes, target, Some(value) if it must decode to it, None if it
/// must be refused with 422)`.
const GATING: &[(&str, &str, &str, Option<&str>)] = &[
    ("u64::MAX", "1b ffffffffffffffff", "u64", Some("18446744073709551615")),
    ("i64::MIN", "3b 7fffffffffffffff", "i64", Some("-9223372036854775808")),
    ("i64::MAX", "1b 7fffffffffffffff", "i64", Some("9223372036854775807")),
    ("-1 into i64", "20", "i64", Some("-1")),
    ("255 into u8", "18 ff", "u8", Some("255")),
    ("a non-preferred head", "1b 0000000000000001", "u64", Some("1")),
    ("u64::MAX into i64", "1b ffffffffffffffff", "i64", None),
    ("i64::MIN - 1 into i64", "3b 8000000000000000", "i64", None),
    ("-2^64 into i64", "3b ffffffffffffffff", "i64", None),
    ("-1 into u64", "20", "u64", None),
    ("256 into u8", "19 0100", "u8", None),
    ("a float into u64", "f9 3c00", "u64", None),
    ("text into u64", "61 31", "u64", None),
];

/// Recorded only: what #989 asks to be listed.
const RECORDED: &[(&str, &str, &str)] = &[
    ("-2^64 into i128", "3b ffffffffffffffff", "i128"),
    ("2^64 (tag 2) into u128", "c2 49 010000000000000000", "u128"),
    ("2^64 (tag 2) into u64", "c2 49 010000000000000000", "u64"),
    ("-2^64 - 1 (tag 3) into i128", "c3 49 010000000000000000", "i128"),
    ("1 as a tag-2 bignum into u64", "c2 41 01", "u64"),
    ("u64::MAX into u128", "1b ffffffffffffffff", "u128"),
    (
        "2^128 (tag 2) into u128",
        "c2 51 0100000000000000000000000000000000",
        "u128",
    ),
];

/// Run the section.
#[must_use]
pub fn run<B: Backend>() -> Section {
    let mut section = Section::new("integers");
    for (what, text, target, expected) in GATING {
        let case = format!("{what} ({text})#{target}");
        section.case();
        let outcome = decode::<B>(target, &hex(text));
        let right = match (expected, &outcome) {
            (Some(value), Outcome::Accept(got)) => match got {
                Value::Number(number) => number.to_string() == *value,
                Value::String(text) => text == value,
                _ => false,
            },
            (None, Outcome::Reject(failure)) => failure.status() == 422,
            _ => false,
        };
        if !right {
            let want = expected.map_or_else(|| json!("refused, 422"), |v| json!(v));
            section.differ(&case, want, outcome.describe());
            section.fail(
                &case,
                match (expected, &outcome) {
                    (Some(_), Outcome::Accept(_)) => "another value",
                    (Some(_), _) => "an integer in range refused",
                    (None, Outcome::Accept(_)) => "an integer out of range accepted",
                    (None, Outcome::Reject(_)) => "out of range answered 400, not 422",
                    (None, Outcome::Panic(_)) => "panicked",
                },
            );
        }
    }
    let mut observed = serde_json::Map::new();
    for (what, text, target) in RECORDED {
        section.case();
        let outcome = decode::<B>(target, &hex(text));
        if let Outcome::Panic(message) = &outcome {
            section.fail(&format!("{what}#{target}"), format!("panicked: {message}"));
        }
        observed.insert(format!("{what}#{target}"), outcome.describe());
    }
    section.data("beyond_64_bits", Value::Object(observed));
    section
}
