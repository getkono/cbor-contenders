//! Section `structs`: what a derived struct and a map do with keys.
//!
//! Gates: `deny_unknown_fields` must refuse an unknown member with 422, and a
//! member a struct skips must still be a well-formed item (an ill-formed one
//! is a 400, not skipped past). Recorded: what a repeated key does in a
//! derived struct and in a map (RFC 8949 section 5.6 calls such a map
//! invalid, but leaves the decoder's answer to it), and whether a struct
//! reads its fields in any order.

use std::collections::BTreeMap;

use codecs::Backend;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::cases::hex;
use crate::outcome::{ARRIVALS, Outcome, Typed, owned};
use crate::record::Section;

/// A struct with two members.
#[derive(Debug, Serialize, Deserialize)]
pub struct Pair {
    /// First.
    pub a: u8,
    /// Second.
    pub b: u8,
}

/// [`Pair`], refusing unknown members.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Strict {
    /// First.
    pub a: u8,
}

/// Run the section.
#[must_use]
pub fn run<B: Backend>() -> Section {
    let mut section = Section::new("structs");
    let gating: &[(&str, &str, crate::outcome::Decode, u16)] = &[
        (
            "an unknown member under deny_unknown_fields",
            "a2 61 61 01 61 7a 01",
            owned::<B, Typed<Strict>>,
            422,
        ),
        (
            "a skipped member that is truncated",
            "a3 61 61 01 61 62 02 61 7a 82 01",
            owned::<B, Typed<Pair>>,
            400,
        ),
        (
            "a skipped member with a reserved head",
            "a3 61 61 01 61 62 02 61 7a 1c",
            owned::<B, Typed<Pair>>,
            400,
        ),
        (
            "a skipped member with a stray break",
            "a3 61 61 01 61 62 02 61 7a ff",
            owned::<B, Typed<Pair>>,
            400,
        ),
        ("a missing member", "a1 61 61 01", owned::<B, Typed<Pair>>, 422),
    ];
    for (what, text, decode, status) in gating {
        for arrival in ARRIVALS {
            let case = format!("{what}#{}", arrival.name());
            section.case();
            match decode(&hex(text), arrival) {
                Outcome::Reject(failure) if failure.status() == *status => {}
                other => {
                    section.differ(&case, json!(format!("refused, {status}")), other.describe());
                    section.fail(&case, format!("expected a refusal with {status}"));
                }
            }
        }
    }
    let recorded: Vec<(&str, &str, crate::outcome::Decode)> = vec![
        (
            "repeated key in a struct",
            "a3 61 61 01 61 62 02 61 61 03",
            owned::<B, Typed<Pair>>,
        ),
        (
            "repeated key in a map",
            "a2 61 61 01 61 61 02",
            owned::<B, Typed<BTreeMap<String, u8>>>,
        ),
        (
            "members in reverse order",
            "a2 61 62 02 61 61 01",
            owned::<B, Typed<Pair>>,
        ),
        ("a struct as an array", "82 01 02", owned::<B, Typed<Pair>>),
    ];
    let mut observed = serde_json::Map::new();
    for (what, text, decode) in recorded {
        section.case();
        observed.insert(what.to_owned(), decode(&hex(text), ARRIVALS[0]).describe());
    }
    section.data("observed", serde_json::Value::Object(observed));
    section
}
