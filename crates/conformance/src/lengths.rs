//! Section `lengths`: a head declaring far more items or bytes than the body
//! holds must be refused (400) without allocating the declared size.
//!
//! The `conform` binary runs under a counting allocator; each decode's peak
//! live allocation is read, and more than [`ALLOWED`] for a body of a few
//! bytes gates, as does accepting it. (A decoder that reserved what a head
//! declares is one request away from an out-of-memory abort.) Where the
//! target's type does not match the body's shape the body has two faults,
//! and which one a library reports first is precedence: the status is gated
//! only on the targets the whole body fits (it must be 400), and recorded
//! otherwise. Every body is a well-formed prefix of a body of its target's
//! shape (map keys are text, an array of maps holds maps), so a lazy decoder
//! that reads item by item meets the end of input, not a wrong type.

use std::collections::BTreeMap;

use codecs::Backend;
use serde_json::json;

use crate::cases::{HexBytes, hex};
use crate::outcome::{ARRIVALS, AsValue, Decode, Ignored, Outcome, Typed, owned};
use crate::record::Section;

/// Peak bytes a decode of one of these bodies may allocate.
///
/// serde's own `Vec` visitor reserves what a sequence's size hint says, up
/// to 1 MiB (`MAX_PREALLOC_BYTES` in `serde_core`'s `size_hint::cautious`), so
/// every library that passes the head's count as the hint (the oracle
/// included) peaks a few bytes over 1 MiB on `vec-u64`: serde's choice, the
/// same under every library. Twice that, plus room, still tells it apart
/// from reserving the declared size, which is at least 4 GiB here.
pub const ALLOWED: u64 = 4 << 20;

/// Heads declaring 2^32 and 2^63 items or bytes, each over a body of a few
/// bytes, and the targets (besides `value` and `ignored`, which fit every
/// body) whose shape the body is a prefix of.
const CASES: &[(&str, &str, &[&str])] = &[
    ("byte string of 2^32 bytes", "5a ffffffff 00", &["bytes"]),
    ("byte string of 2^63 bytes", "5b 7fffffffffffffff 00", &["bytes"]),
    ("text string of 2^32 bytes", "7a ffffffff 61", &["string"]),
    ("text string of 2^63 bytes", "7b 7fffffffffffffff 61", &["string"]),
    ("array of 2^32 items", "9a ffffffff 00", &["vec-u64"]),
    ("array of 2^63 items", "9b 7fffffffffffffff 00", &["vec-u64"]),
    ("map of 2^32 entries", "ba ffffffff 61 61 00", &["map"]),
    ("map of 2^63 entries", "bb 7fffffffffffffff 61 61 00", &["map"]),
    ("array of 2^32 maps", "9a ffffffff a0", &["vec-map"]),
    (
        "chunk of 2^32 bytes in an indefinite string",
        "5f 5a ffffffff 00 ff",
        &["bytes"],
    ),
];

/// Run the section.
#[must_use]
pub fn run<B: Backend>() -> Section {
    let mut section = Section::new("lengths");
    let mut peaks = serde_json::Map::new();
    let mut precedence = serde_json::Map::new();
    let paths: [(&str, Decode); 7] = [
        ("bytes", owned::<B, HexBytes>),
        ("string", owned::<B, Typed<String>>),
        ("vec-u64", owned::<B, Typed<Vec<u64>>>),
        ("map", owned::<B, Typed<BTreeMap<String, u64>>>),
        ("vec-map", owned::<B, Typed<Vec<BTreeMap<String, u64>>>>),
        ("value", owned::<B, AsValue>),
        ("ignored", owned::<B, Ignored>),
    ];
    for (what, text, fitting) in CASES {
        let bytes = hex(text);
        for (target, decode) in paths {
            let fits = matches!(target, "value" | "ignored") || fitting.contains(&target);
            let case = format!("{what}#{target}");
            section.case();
            alloc_count::reset_peak();
            let before = alloc_count::Snapshot::now();
            let outcome = decode(&bytes, ARRIVALS[0]);
            let peak = alloc_count::peak().saturating_sub(before.live);
            peaks.insert(case.clone(), json!(peak));
            if peak > ALLOWED {
                section.differ(&case, json!(format!("at most {ALLOWED} bytes")), json!(peak));
                section.fail(&case, format!("allocated {peak} bytes for a {}-byte body", bytes.len()));
            }
            if !fits {
                precedence.insert(case.clone(), outcome.describe());
            }
            match outcome {
                Outcome::Reject(failure) if failure.status() == 400 || !fits => {}
                other => {
                    section.differ(&case, json!("refused, 400"), other.describe());
                    section.fail(&case, "a body shorter than its head declares, not refused with 400");
                }
            }
        }
    }
    section.data("peak_bytes", serde_json::Value::Object(peaks));
    section.data("outside_target", serde_json::Value::Object(precedence));
    section
}
