//! Section `deterministic`: what each encoder emits by default, and whether
//! it is byte-stable (RFC 8949 sections 4.1 and 4.2).
//!
//! Gates: encoding the same value twice in one process giving different
//! bytes (Kynos's documents are byte-stable; a response body should not be
//! the exception).
//!
//! Recorded:
//!
//! - whether each workload's output is preferred serialization (shortest
//!   heads and floats, definite lengths);
//! - whether each workload's output, and a `HashMap`'s, is the same in two
//!   processes (std's `HashMap` iterates in a per-process order, so only an
//!   encoder that sorts map keys makes it stable; it is the application's
//!   choice of type);
//! - the key order the encoder writes for a map and a struct whose order
//!   differs from core deterministic encoding's (section 4.2.1: keys sorted
//!   by their encodings, bytewise), which no serde encoder applies by default:
//!   `BTreeMap` order is the strings' order (`"aa"` before `"b"`) and a struct
//!   writes its fields in declaration order, while the encodings sort `"b"`
//!   (`61 62`) before `"aa"` (`62 61 61`);
//! - the library's opt-in deterministic mode, if it has one, which the
//!   adapter does not use (each adapter is the library's defaults).

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::process::Command;

use codecs::{Backend, Visit};
use payloads::Workload;
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::outcome::guarded;
use crate::record::Section;

/// The map whose cross-process stability is recorded.
#[must_use]
pub fn hash_map() -> HashMap<String, u32> {
    (0..64).map(|i| (format!("key-{i:02}"), i)).collect()
}

/// The name [`hash_map`] is listed under beside the workloads.
pub const HASH_MAP: &str = "hash-map";

/// Each library's opt-in deterministic mode, as read from its source at the
/// pinned version: `(crate, entry points)`. A crate not listed has none.
pub const OPT_IN_MODES: &[(&str, &str)] = &[
    (
        "c2pa_cbor",
        "to_vec_deterministic, to_writer_deterministic, Encoder::set_deterministic (RFC 8949 section 4.2.1)",
    ),
    (
        "cbor2",
        "to_canonical_vec, to_canonical_writer (section 4.2.1), and *_with(KeyOrder::LengthFirst) (section 4.2.3)",
    ),
];

/// A struct declaring `aa` before `b`: declaration order is not bytewise
/// order of the encoded keys.
#[derive(Debug, Serialize)]
struct Declared {
    aa: u8,
    b: u8,
}

/// The order an encoding writes `"aa"` and `"b"` in, as the report names it.
fn key_order(bytes: &[u8], as_given: &[u8], bytewise: &[u8]) -> Value {
    let order = if bytes == as_given {
        "as given (BTreeMap order, or declaration order)"
    } else if bytes == bytewise {
        "bytewise (RFC 8949 section 4.2.1)"
    } else {
        "another order or encoding"
    };
    json!({ "order": order, "hex": hex::encode(bytes) })
}

/// The SHA-256 of `B`'s encoding of every workload whose bytes encode a
/// value, and of [`hash_map`]: `(name, digest or why not)`.
fn digests<B: Backend>() -> Vec<(String, String)> {
    struct Digests<B>(std::marker::PhantomData<B>);
    impl<B: Backend> payloads::Visit for Digests<B> {
        type Output = Option<String>;
        fn visit<W: Workload>(self) -> Option<String> {
            (W::EXPECT == payloads::Expect::Value).then(|| digest(B::encode(&W::value())))
        }
    }
    fn digest(encoded: Result<Vec<u8>, codecs::Failure>) -> String {
        encoded.map_or_else(
            |failure| format!("refused: {failure}"),
            |bytes| hex::encode(Sha256::digest(bytes)),
        )
    }
    let mut out: Vec<(String, String)> = payloads::ALL
        .iter()
        .filter_map(|name| {
            payloads::dispatch(name, Digests::<B>(std::marker::PhantomData))
                .flatten()
                .map(|digest| ((*name).to_owned(), digest))
        })
        .collect();
    out.push((HASH_MAP.to_owned(), digest(B::encode(&hash_map()))));
    out
}

/// The `encode-digests <backend>` subcommand: one `<name> <sha256>` line per
/// workload and for [`hash_map`], on stdout. The encodes run on a thread
/// with the suite's stack, so the deepest workload cannot overflow it.
/// Returns the exit code.
#[must_use]
pub fn child(args: &[String]) -> u8 {
    struct Encode;
    impl Visit for Encode {
        type Output = u8;
        fn visit<B: Backend>(self) -> u8 {
            let spawned = std::thread::Builder::new()
                .stack_size(crate::SUITE_STACK)
                .spawn(digests::<B>)
                .map(std::thread::JoinHandle::join);
            let Ok(Ok(lines)) = spawned else { return 1 };
            for (name, digest) in lines {
                println!("{name} {digest}");
            }
            0
        }
    }
    let [backend] = args else { return 2 };
    codecs::dispatch(backend, Encode).unwrap_or(3)
}

struct Stable<'s, B> {
    section: &'s mut Section,
    preferred: &'s mut serde_json::Map<String, Value>,
    backend: std::marker::PhantomData<B>,
}

impl<B: Backend> payloads::Visit for Stable<'_, B> {
    type Output = ();

    fn visit<W: Workload>(self) {
        if W::EXPECT != payloads::Expect::Value {
            return;
        }
        let value = W::value();
        self.section.case();
        let (Ok(Ok(first)), Ok(Ok(second))) = (guarded(|| B::encode(&value)), guarded(|| B::encode(&value))) else {
            // `encode_diff` reports an encode that fails.
            return;
        };
        if first != second {
            self.section
                .differ(W::NAME, json!("the same bytes twice"), json!("different bytes"));
            self.section.fail(W::NAME, "two encodings of one value differ");
        }
        self.preferred
            .insert(W::NAME.to_owned(), json!(faithful::preferred(&first)));
    }
}

/// The digests one child printed, by name.
fn read_digests(stdout: &str) -> BTreeMap<String, String> {
    stdout
        .lines()
        .filter_map(|line| line.split_once(' '))
        .map(|(name, digest)| (name.to_owned(), digest.to_owned()))
        .collect()
}

/// Run the section, re-executing `exe` for the cross-process check; without
/// one that check is skipped.
#[must_use]
pub fn run<B: Backend>(exe: Option<&Path>) -> Section {
    let mut section = Section::new("deterministic");
    let mode = OPT_IN_MODES
        .iter()
        .find(|(name, _)| *name == B::CRATE)
        .map(|(_, entry)| *entry);
    section.data(
        "opt_in_mode",
        json!({ "library_has": mode, "used": false, "why": "each adapter is the library's defaults" }),
    );
    if !B::ENCODES {
        section.note("decode-only backend: nothing to encode");
        return section;
    }
    let mut preferred = serde_json::Map::new();
    for name in payloads::ALL {
        payloads::dispatch(
            name,
            Stable::<B> {
                section: &mut section,
                preferred: &mut preferred,
                backend: std::marker::PhantomData,
            },
        );
    }
    let all = preferred.values().all(|p| p == &json!(true));
    section.data("preferred", json!({ "all": all, "by_workload": preferred }));
    // `{"aa": 1, "b": 2}` as given, and with the keys in bytewise order.
    let as_given = hex_bytes("a2 62 61 61 01 61 62 02");
    let bytewise = hex_bytes("a2 61 62 02 62 61 61 01");
    let map = BTreeMap::from([("aa".to_owned(), 1u8), ("b".to_owned(), 2u8)]);
    let declared = Declared { aa: 1, b: 2 };
    let mut order = serde_json::Map::new();
    for (what, observed) in [
        ("BTreeMap<String, u8>", encoded::<B, _>(&map)),
        ("struct", encoded::<B, _>(&declared)),
    ] {
        section.case();
        let observed = observed.map_or_else(|why| why, |bytes| key_order(&bytes, &as_given, &bytewise));
        order.insert(what.to_owned(), observed);
    }
    section.data("key_order", Value::Object(order));
    if let Some(exe) = exe {
        let run = || {
            Command::new(exe)
                .args(["encode-digests", B::NAME])
                .output()
                .ok()
                .filter(|out| out.status.success())
                .map(|out| read_digests(&String::from_utf8_lossy(&out.stdout)))
        };
        let across = match (run(), run()) {
            (Some(a), Some(b)) => {
                let by_name: serde_json::Map<String, Value> = a
                    .iter()
                    .map(|(name, digest)| {
                        section.case();
                        (name.clone(), json!(b.get(name) == Some(digest)))
                    })
                    .collect();
                let stable = by_name.values().all(|stable| stable == &json!(true));
                json!({ "all_stable": stable, "by_workload": by_name })
            }
            _ => json!({ "all_stable": null, "detail": "the probe did not run" }),
        };
        section.data("across_processes", across);
    }
    section
}

fn hex_bytes(text: &str) -> Vec<u8> {
    crate::cases::hex(text)
}

/// `B`'s encoding of `value`, or the report's rendering of why not.
fn encoded<B: Backend, T: Serialize>(value: &T) -> Result<Vec<u8>, Value> {
    match guarded(|| B::encode(value)) {
        Ok(Ok(bytes)) => Ok(bytes),
        Ok(Err(failure)) => Err(json!({ "refused": failure.to_string() })),
        Err(panic) => Err(json!({ "panic": panic })),
    }
}
