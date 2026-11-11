//! The suite's own rules, checked on backends whose answers are known: the
//! oracle, and the oracle made to stop borrowing in each of the two ways a
//! candidate does.

use bytes::Bytes;
use codecs::backend::serde_cbor::SerdeCbor;
use codecs::{Backend, Body, Class, Failure, Format, Trailing};
use conformance::record::Section;
use conformance::{attributes, depth, deterministic, lengths, strings};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

#[global_allocator]
static ALLOCATOR: alloc_count::Counting = alloc_count::Counting;

fn failure(error: &serde_cbor::Error) -> Failure {
    use serde_cbor::error::Category;
    let class = match error.classify() {
        Category::Io => Class::Io,
        Category::Syntax => Class::Syntax,
        Category::Data => Class::Data,
        Category::Eof => Class::Eof,
    };
    Failure::new(class, error)
}

/// The oracle, with a borrowed decode that reads through `std::io::Read`, as
/// c2pa_cbor's `from_slice` does: it never hands out a slice of the input, so
/// a `&str` is refused with 422.
#[derive(Debug)]
struct Reader;

/// The oracle, with no borrowing entry point at all, as ciborium.
#[derive(Debug)]
struct NoBorrow;

macro_rules! oracle_but_borrowed {
    ($name:ident, $label:literal, |$body:ident| $borrowed:expr) => {
        impl Backend for $name {
            const NAME: &'static str = $label;
            const CRATE: &'static str = "serde_cbor";
            const FORMAT: Format = Format::Cbor;
            const ENCODES: bool = true;
            const ELIGIBLE: bool = false;
            const TRAILING: Trailing = Trailing::Refused;

            fn decode<T: DeserializeOwned>(body: Bytes) -> Result<T, Failure> {
                SerdeCbor::decode(body)
            }

            fn decode_borrowed<'a, T: Deserialize<'a>>($body: &'a mut Body) -> Result<T, Failure> {
                $borrowed
            }

            fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, Failure> {
                SerdeCbor::encode(value)
            }

            fn encode_into<T: Serialize>(value: &T, out: &mut Vec<u8>) -> Result<(), Failure> {
                SerdeCbor::encode_into(value, out)
            }

            fn to_writer<T: Serialize, W: std::io::Write>(value: &T, out: W) -> Result<(), Failure> {
                SerdeCbor::to_writer(value, out)
            }
        }
    };
}

oracle_but_borrowed!(Reader, "reader", |body| {
    let mut de = serde_cbor::Deserializer::new(serde_cbor::de::IoRead::new(body.as_slice()));
    let value = T::deserialize(&mut de).map_err(|e| failure(&e))?;
    de.end().map_err(|e| failure(&e))?;
    Ok(value)
});

oracle_but_borrowed!(NoBorrow, "no-borrow", |_body| Err(Failure::unsupported()));

fn failed_cases(section: &Section) -> Vec<String> {
    section
        .failures()
        .iter()
        .map(|failure| failure["case"].as_str().unwrap_or_default().to_owned())
        .collect()
}

/// The oracle's own defects, which the suite finds in it like in any
/// library. It validates an indefinite-length text string's UTF-8 after
/// joining the chunks, so a character split across two chunks passes (RFC
/// 8949 section 3.2.3 makes each chunk a text string of its own); and it
/// reads `{"New":1,"Unit":null}` into an externally tagged enum, which
/// serde_json refuses.
const ORACLE_DEFECTS: &[&str] = &["split sequence across chunks", "externally-tagged/6#"];

fn known_defect(case: &str) -> bool {
    ORACLE_DEFECTS.iter().any(|defect| case.starts_with(defect))
}

/// The oracle borrows every borrowing target, and fails only its known
/// defects.
#[test]
fn the_oracle_borrows_and_fails_only_its_known_defects() {
    for section in [strings::run::<SerdeCbor>(), attributes::run::<SerdeCbor>()] {
        assert_eq!(section.not_borrowed_count(), 0, "{}", section.name());
        let failed = failed_cases(&section);
        assert!(failed.iter().all(|case| known_defect(case)), "{failed:#?}");
    }
}

/// A library that cannot borrow is recorded as not borrowing, the same way
/// whichever way it fails to: never gated, and never counted as a case it
/// passed. What is left is the oracle's own answer on the targets the
/// library does produce.
#[test]
fn not_borrowing_is_recorded_alike_and_never_gated() {
    for (oracle, sections) in [
        (
            strings::run::<SerdeCbor>(),
            [strings::run::<Reader>(), strings::run::<NoBorrow>()],
        ),
        (
            attributes::run::<SerdeCbor>(),
            [attributes::run::<Reader>(), attributes::run::<NoBorrow>()],
        ),
    ] {
        let oracle_failed = failed_cases(&oracle);
        for section in sections {
            assert!(section.not_borrowed_count() > 0, "{}", section.name());
            // Every case the oracle was judged on is judged or not borrowed.
            assert_eq!(section.cases() + section.not_borrowed_count(), oracle.cases());
            for case in failed_cases(&section) {
                assert!(oracle_failed.contains(&case) && !case.contains("#&str#"), "{case}");
            }
        }
    }
}

/// The oracle is a lazy decoder that reads item by item and allocates only
/// what it has read: every body is refused with 400 on its own target, and
/// nothing is allocated near the declared size. A body that fails a correct
/// lazy decoder would be a case judging precedence, not length.
#[test]
fn the_oracle_passes_lengths() {
    let section = lengths::run::<SerdeCbor>();
    assert!(section.cases() > 0);
    assert_eq!(section.failure_count(), 0, "{:#?}", section.failures());
}

/// Every depth probe, the recursive struct, indefinite nesting and the
/// `nested-*` workloads included, runs in a child of the `conform` binary.
/// The oracle accepts 127 levels and refuses 128 on every shape and target,
/// and never crashes decoding.
#[test]
fn the_oracle_refuses_past_its_depth_on_every_shape() {
    let exe = std::path::Path::new(env!("CARGO_BIN_EXE_conform"));
    let probes = depth::probe_all(exe, SerdeCbor::NAME);
    for (job, probe) in depth::jobs().into_iter().zip(&probes) {
        let what = format!("{job:?}: {probe:?}");
        match job.target {
            depth::Decoded::Encode => {}
            depth::Decoded::Owned | depth::Decoded::Borrowed => {
                let expected = if job.depth < 128 {
                    depth::Probe::Accepted
                } else {
                    depth::Probe::Rejected(400)
                };
                assert_eq!(*probe, expected, "{what}");
            }
            _ if job.depth < 128 => assert_eq!(*probe, depth::Probe::Accepted, "{what}"),
            _ => assert_eq!(*probe, depth::Probe::Rejected(400), "{what}"),
        }
    }
    assert_eq!(probes.len(), depth::jobs().len());
}

/// The oracle writes keys as given (no sorting), every workload the same
/// way in two processes, and has no opt-in deterministic mode.
#[test]
fn the_oracle_is_stable_and_writes_keys_as_given() {
    let exe = std::path::Path::new(env!("CARGO_BIN_EXE_conform"));
    let section = deterministic::run::<SerdeCbor>(Some(exe));
    assert_eq!(section.failure_count(), 0, "{:#?}", section.failures());
    let (record, _) = section.finish();
    let data = &record["data"];
    assert_eq!(data["opt_in_mode"]["library_has"], serde_json::Value::Null);
    for what in ["BTreeMap<String, u8>", "struct"] {
        let order = data["key_order"][what]["order"].as_str().unwrap_or_default();
        assert!(order.starts_with("as given"), "{what}: {order}");
    }
    let across = data["across_processes"]["by_workload"].as_object().expect("digests");
    assert!(across.len() > 30, "{across:?}");
    for (name, stable) in across {
        if name != deterministic::HASH_MAP {
            assert_eq!(stable, &serde_json::json!(true), "{name}");
        }
    }
}
