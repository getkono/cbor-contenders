//! Section `deterministic`: what each encoder emits by default, and whether
//! it is byte-stable (RFC 8949 sections 4.1 and 4.2).
//!
//! Gates: encoding the same value twice in one process giving different
//! bytes (Kynos's documents are byte-stable; a response body should not be
//! the exception). Recorded: whether each workload's output is preferred
//! serialization (shortest heads and floats, definite lengths), and whether a
//! `HashMap`'s encoding is the same in two processes (it is the application's
//! choice of type, and std's `HashMap` iterates in a per-process order; an
//! encoder that sorts map keys makes it stable). Core deterministic encoding
//! (section 4.2.1) also sorts map keys, which no serde encoder of a derived
//! struct does by default: a struct's fields are written in declaration
//! order. Where a library has an opt-in deterministic mode, the adapter does
//! not use it (each adapter is the library's defaults).

use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

use codecs::{Backend, Visit};
use payloads::Workload;
use serde_json::{Value, json};

use crate::outcome::guarded;
use crate::record::Section;

/// The map whose cross-process stability is recorded.
#[must_use]
pub fn hash_map() -> HashMap<String, u32> {
    (0..64).map(|i| (format!("key-{i:02}"), i)).collect()
}

/// The `encode-hashmap <backend>` subcommand: the backend's encoding of
/// [`hash_map`] in hex on stdout. Returns the exit code.
#[must_use]
pub fn child(args: &[String]) -> u8 {
    struct Encode;
    impl Visit for Encode {
        type Output = u8;
        fn visit<B: Backend>(self) -> u8 {
            match B::encode(&hash_map()) {
                Ok(bytes) => {
                    println!("{}", hex::encode(bytes));
                    0
                }
                Err(_) => 1,
            }
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

/// Run the section, re-executing `exe` for the cross-process check; without
/// one that check is skipped.
#[must_use]
pub fn run<B: Backend>(exe: Option<&Path>) -> Section {
    let mut section = Section::new("deterministic");
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
    if let Some(exe) = exe {
        let run = || {
            Command::new(exe)
                .args(["encode-hashmap", B::NAME])
                .output()
                .ok()
                .filter(|out| out.status.success())
                .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_owned())
        };
        let (a, b) = (run(), run());
        section.case();
        section.data(
            "hash_map_across_processes",
            match (a, b) {
                (Some(a), Some(b)) => json!({ "stable": a == b }),
                _ => json!({ "stable": null, "detail": "the probe did not run" }),
            },
        );
    }
    section
}
