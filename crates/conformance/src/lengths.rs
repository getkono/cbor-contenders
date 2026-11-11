//! Section `lengths`: a head declaring far more items or bytes than the body
//! holds must be refused (400) without allocating the declared size.
//!
//! The `conform` binary runs under a counting allocator; each decode's peak
//! live allocation is read, and more than [`ALLOWED`] for a body of a few
//! bytes gates, as does accepting it. (A decoder that reserved what a head
//! declares is one request away from an out-of-memory abort.) Where the
//! target's type does not match the head's major type the body has two
//! faults, and which one a library reports first is precedence: the status
//! is gated only where the target fits the head (it must be 400), and
//! recorded otherwise.

use std::collections::BTreeMap;

use codecs::Backend;
use serde_json::json;

use crate::cases::{HexBytes, hex};
use crate::outcome::{ARRIVALS, AsValue, Decode, Ignored, Outcome, Typed, owned};
use crate::record::Section;

/// Peak bytes a decode of one of these bodies may allocate.
pub const ALLOWED: u64 = 1 << 20;

/// Heads declaring 2^32 and 2^63 items or bytes, each over a body of a few
/// bytes.
const CASES: &[(&str, &str)] = &[
    ("byte string of 2^32 bytes", "5a ffffffff 00"),
    ("byte string of 2^63 bytes", "5b 7fffffffffffffff 00"),
    ("text string of 2^32 bytes", "7a ffffffff 61"),
    ("text string of 2^63 bytes", "7b 7fffffffffffffff 61"),
    ("array of 2^32 items", "9a ffffffff 00"),
    ("array of 2^63 items", "9b 7fffffffffffffff 00"),
    ("map of 2^32 entries", "ba ffffffff 00 00"),
    ("map of 2^63 entries", "bb 7fffffffffffffff 00 00"),
    ("array of 2^32 maps", "9a ffffffff a0"),
    ("chunk of 2^32 bytes in an indefinite string", "5f 5a ffffffff 00 ff"),
];

/// Run the section.
#[must_use]
pub fn run<B: Backend>() -> Section {
    let mut section = Section::new("lengths");
    let mut peaks = serde_json::Map::new();
    // Each path, and the heads (by their first byte's major type) it fits.
    let paths: [(&str, Decode, &[u8]); 6] = [
        ("bytes", owned::<B, HexBytes>, &[2]),
        ("string", owned::<B, Typed<String>>, &[3]),
        ("vec-u64", owned::<B, Typed<Vec<u64>>>, &[4]),
        ("map", owned::<B, Typed<BTreeMap<String, u64>>>, &[5]),
        ("value", owned::<B, AsValue>, &[2, 3, 4, 5]),
        ("ignored", owned::<B, Ignored>, &[2, 3, 4, 5]),
    ];
    for (what, text) in CASES {
        let bytes = hex(text);
        for (target, decode, fits) in paths {
            let fits = fits.contains(&(bytes[0] >> 5));
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
    section
}
