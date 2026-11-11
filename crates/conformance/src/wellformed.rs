//! Section `wellformed`: bodies RFC 8949 section 5.1 calls ill-formed, which
//! a decoder must refuse, and a server must answer 400.
//!
//! Each is decoded into `IgnoredAny` (the path that skips, where a lazy
//! decoder could accept without looking) and into `serde_json::Value` (the
//! path that reads everything), through both arrivals. Accepting one, or
//! refusing it with 422, gates. Trailing bytes are the one case a library may
//! leave to its caller by API design ("decode the first item"); they gate
//! too, because a body under `application/cbor` is one item, and whether the
//! library's entry point checks is recorded beside it.

use codecs::Backend;
use serde_json::json;

use crate::cases::hex;
use crate::outcome::{ARRIVALS, AsValue, Ignored, Outcome, owned};
use crate::record::Section;

/// Ill-formed bodies, each with what makes it so.
pub const CASES: &[(&str, &str)] = &[
    ("", "empty body"),
    ("18", "truncated argument"),
    ("1b 0000", "truncated 8-byte argument"),
    ("62 61", "text shorter than declared"),
    ("44 0102", "bytes shorter than declared"),
    ("82 01", "array shorter than declared"),
    ("a1 61 61", "map missing a value"),
    ("1c", "additional information 28"),
    ("3d", "additional information 29 on a negative integer"),
    ("5e", "additional information 30 on a byte string"),
    ("fe", "additional information 30 on a simple value"),
    ("ff", "break outside an indefinite item"),
    ("82 01 ff", "break inside a definite array"),
    ("1f", "indefinite length on an integer"),
    ("3f", "indefinite length on a negative integer"),
    ("df 01", "indefinite length on a tag"),
    ("7f 01 ff", "integer chunk in an indefinite text string"),
    ("7f 41 00 ff", "byte-string chunk in an indefinite text string"),
    ("5f 61 00 ff", "text chunk in an indefinite byte string"),
    ("7f 7f ff ff", "nested indefinite chunk"),
    ("7f 61 61", "indefinite text string with no break"),
    ("9f 01", "indefinite array with no break"),
    ("bf 61 61 ff", "indefinite map with a key and no value"),
    ("f8 18", "two-byte simple value 24"),
    ("f8 1f", "two-byte simple value 31"),
    ("01 01", "a second item after the body"),
    ("a0 00", "bytes after an empty map"),
];

/// Run the section.
#[must_use]
pub fn run<B: Backend>() -> Section {
    let mut section = Section::new("wellformed");
    let mut trailing = Vec::new();
    for (text, what) in CASES {
        let bytes = hex(text);
        for (target, outcome) in [
            ("ignored", owned::<B, Ignored> as crate::outcome::Decode),
            ("value", owned::<B, AsValue>),
        ]
        .map(|(name, decode)| (name, ARRIVALS.map(|arrival| decode(&bytes, arrival))))
        {
            for (outcome, arrival) in outcome.iter().zip(ARRIVALS) {
                let case = format!("{what} ({text})#{target}#{}", arrival.name());
                section.case();
                match outcome {
                    Outcome::Reject(failure) if failure.status() == 400 => {}
                    other => {
                        section.differ(&case, json!("refused, 400"), other.describe());
                        let detail = match other {
                            Outcome::Accept(_) => "an ill-formed body accepted",
                            Outcome::Reject(_) => "an ill-formed body answered 422, not 400",
                            Outcome::Panic(_) => "panicked",
                        };
                        section.fail(&case, detail);
                    }
                }
                if what.contains("after") && arrival.name() == "unique" {
                    trailing.push(json!({ "case": text, "target": target, "outcome": outcome.describe() }));
                }
            }
        }
    }
    section.data("trailing", json!({ "library": B::TRAILING, "outcomes": trailing }));
    section
}
