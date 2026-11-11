use std::borrow::Cow;

use codecs::Format;
use sha2::{Digest, Sha256};

use super::*;

fn sha(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// kynos-bench's golden bodies (`crates/bench-app/golden/*.http` at 5bd67bc),
/// copied as digests because that repository is private (the digests
/// serde-json-contenders pins). Equality proves the three values are the ones
/// a Kynos server is benchmarked on: their JSON is byte for byte the golden
/// body, and every other format encodes the same value.
#[test]
fn kynos_shapes_match_kynos_bench_golden_bodies() {
    assert_eq!(
        sha(&kynos::JsonSmall::bytes(Format::Json)),
        "aca6359372f90e9b237687e8e64f42c49a0b67469049fc9c33c0e28d38147058"
    );
    assert_eq!(
        sha(&kynos::JsonLarge::bytes(Format::Json)),
        "932d688ff96de76bb674d9012898e43539ea7fab821f88689994fb32f6c4dd7a"
    );
    assert_eq!(
        sha(&kynos::EchoPost::bytes(Format::Json)),
        "d4eaa3b6cc7740ca8814e8192fa200e6ccf9f48ed9d6e630ccc1f6a6b11c5816"
    );
}

/// How far serde_json's default float parser may land from the correctly
/// rounded value (it is not correctly rounded; `float_roundtrip` is). CBOR
/// and protobuf floats are bit patterns, held exactly.
const JSON_DECODE_ULPS: u64 = 4;

fn ulps(a: f64, b: f64) -> u64 {
    let key = |x: f64| {
        let bits = x.to_bits() as i64;
        if bits < 0 { -(bits & i64::MAX) - 1 } else { bits }
    };
    key(a).abs_diff(key(b))
}

/// The largest float distance between two values of the same shape, or
/// `None` if anything but a float differs.
fn max_ulps(got: &serde_json::Value, expected: &serde_json::Value) -> Option<u64> {
    use serde_json::Value;
    match (got, expected) {
        (Value::Number(g), Value::Number(e)) if g.is_f64() && e.is_f64() => Some(ulps(g.as_f64()?, e.as_f64()?)),
        (Value::Array(g), Value::Array(e)) if g.len() == e.len() => g
            .iter()
            .zip(e)
            .try_fold(0, |max, (g, e)| Some(max_ulps(g, e)?.max(max))),
        (Value::Object(g), Value::Object(e)) if g.len() == e.len() => e
            .iter()
            .try_fold(0, |max, (key, e)| Some(max_ulps(g.get(key)?, e)?.max(max))),
        (g, e) => (g == e).then_some(0),
    }
}

/// The formats this build can encode: minicbor's derive only with its
/// feature.
fn formats() -> Vec<Format> {
    let mut formats = vec![Format::Json, Format::Cbor, Format::Protobuf];
    if cfg!(feature = "minicbor") {
        formats.push(Format::MinicborNative);
    }
    formats
}

/// Deep enough for serde's recursion through the deepest corpus.
fn on_a_big_stack<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(512 << 20)
        .spawn(f)
        .expect("spawn")
        .join()
        .expect("joins")
}

/// Every corpus decodes back to its value in its format, through a decoder
/// that is not a candidate (serde_json, serde_cbor, prost, minicbor), or is
/// refused with the workload's status; and its digest.
struct Facts(Format);

impl Visit for Facts {
    type Output = Option<(usize, String)>;

    fn visit<W: Workload>(self) -> Option<(usize, String)> {
        let format = self.0;
        let bytes = W::bytes(format);
        let native = matches!(format, Format::Protobuf | Format::MinicborNative);
        if native && W::native().is_none() {
            assert!(bytes.is_empty(), "{} has {format:?} bytes but no native value", W::NAME);
            return None;
        }
        let decoded: Result<serde_json::Value, (u16, String)> = match format {
            Format::Json => serde_json::from_slice::<W::Owned>(&bytes)
                .map(|v| serde_json::to_value(v).expect("a JSON value"))
                .map_err(|e| (codecs::backend::serde_json::failure(&e).status(), e.to_string())),
            Format::Cbor => serde_cbor::from_slice::<W::Owned>(&bytes)
                .map(|v| serde_json::to_value(v).expect("a JSON value"))
                .map_err(|e| (if e.is_data() { 422 } else { 400 }, e.to_string())),
            Format::Protobuf | Format::MinicborNative => {
                let back = if format == Format::Protobuf {
                    W::Native::proto_decode(bytes::Bytes::from(bytes.clone()))
                } else {
                    W::Native::mini_decode(&bytes)
                };
                match back {
                    Ok(v) => {
                        assert_eq!(Some(v), W::native(), "{} {format:?}", W::NAME);
                        Ok(serde_json::Value::Null)
                    }
                    Err(f) => Err((f.status(), f.to_string())),
                }
            }
        };
        match (W::EXPECT, decoded) {
            (Expect::Reject(status), Err((got, why))) => assert_eq!(got, status, "{} {format:?}: {why}", W::NAME),
            (Expect::Reject(_), Ok(_)) => panic!("{} {format:?} decodes", W::NAME),
            (Expect::Value, Err((_, why))) => {
                // serde_json and serde_cbor both stop at 128 levels; the deep
                // corpora are checked by `nested_*` instead.
                assert!(
                    W::NAME.starts_with("nested-") && W::NAME != "nested-100",
                    "{} {format:?}: {why}",
                    W::NAME
                );
            }
            (Expect::Value, Ok(_)) if native => {}
            (Expect::Value, Ok(got)) => {
                let expected = serde_json::to_value(W::value()).expect("a JSON value");
                let tolerance = if format == Format::Json { JSON_DECODE_ULPS } else { 0 };
                let distance = max_ulps(&got, &expected);
                assert!(
                    distance.is_some_and(|d| d <= tolerance),
                    "{} {format:?} decodes to another value ({distance:?})",
                    W::NAME
                );
            }
        }
        Some((bytes.len(), sha(&bytes)))
    }
}

/// The `SHA256SUMS` table this build computes.
fn table() -> String {
    let mut table = String::new();
    for format in formats() {
        for name in ALL {
            let name = *name;
            let facts = on_a_big_stack(move || dispatch(name, Facts(format)).expect("listed workload dispatches"));
            if let Some((len, digest)) = facts {
                table.push_str(&format!("{digest}  {len:>8}  {:<15}  {name}\n", format.name()));
            }
        }
    }
    table
}

/// Every corpus, pinned. A generator change that moves an input fails here
/// and is reviewed as a change to what every result is about. Run with
/// `PAYLOADS_BLESS=1` (and `--features minicbor`) to rewrite the table.
#[test]
fn corpora_are_pinned() {
    let table = table();
    let expected: String = include_str!("../SHA256SUMS")
        .lines()
        .filter(|line| cfg!(feature = "minicbor") || !line.contains("  minicbor-native  "))
        .flat_map(|line| [line, "\n"])
        .collect();
    if std::env::var_os("PAYLOADS_BLESS").is_some() && cfg!(feature = "minicbor") {
        std::fs::write(concat!(env!("CARGO_MANIFEST_DIR"), "/SHA256SUMS"), &table).expect("write SHA256SUMS");
        return;
    }
    assert_eq!(table, expected, "corpus digests moved:\n{table}");
}

#[test]
fn sweep_bodies_are_exactly_their_size_in_cbor() {
    for (name, size) in [
        (sweep::Sweep::<64>::NAME, 64),
        (sweep::Sweep::<256>::NAME, 256),
        (sweep::Sweep::<1024>::NAME, 1024),
        (sweep::Sweep::<4096>::NAME, 4096),
        (sweep::Sweep::<16384>::NAME, 16384),
        (sweep::Sweep::<65536>::NAME, 65536),
        (sweep::Sweep::<262_144>::NAME, 262_144),
        (sweep::Sweep::<1_048_576>::NAME, 1_048_576),
    ] {
        let (len, _) = dispatch(name, Facts(Format::Cbor)).flatten().expect("dispatches");
        assert_eq!(len, size, "{name}");
    }
}

/// Each written-by-hand nested body is what the serializers write for the
/// value, on a stack deep enough for serde's recursion.
fn check_nested<const D: u32>() {
    let (json, cbor) = on_a_big_stack(|| {
        let value = families::Nested::<D>::value();
        (crate::json(&value), crate::cbor(&value))
    });
    assert!(json == families::Nested::<D>::bytes(Format::Json), "{D} JSON");
    assert!(cbor == families::Nested::<D>::bytes(Format::Cbor), "{D} CBOR");
}

#[test]
fn nested_bodies_are_their_values() {
    check_nested::<100>();
    check_nested::<255>();
    check_nested::<257>();
    check_nested::<10_000>();
}

#[test]
fn cbor_writes_bytes_as_byte_strings_and_json_as_base64() {
    let blobs = blob::ByteStrings::value();
    let first = &blobs.blobs[0];
    let json: serde_json::Value = serde_json::from_slice(&blob::ByteStrings::bytes(Format::Json)).expect("JSON");
    let text = json["blobs"][0]["data"].as_str().expect("base64 text");
    assert_eq!(
        base64::Engine::decode(&base64::engine::general_purpose::STANDARD, text).expect("base64"),
        first.data
    );
    let value: serde_cbor::Value = serde_cbor::from_slice(&blob::ByteStrings::bytes(Format::Cbor)).expect("CBOR");
    let serde_cbor::Value::Map(root) = value else {
        panic!("a map")
    };
    let serde_cbor::Value::Array(list) = &root[&serde_cbor::Value::Text("blobs".into())] else {
        panic!("a list")
    };
    let serde_cbor::Value::Map(blob) = &list[0] else {
        panic!("a map")
    };
    assert_eq!(
        blob[&serde_cbor::Value::Text("data".into())],
        serde_cbor::Value::Bytes(first.data.clone())
    );
    // The borrowed form borrows the bytes from a CBOR body.
    let bytes = blob::ByteStrings::bytes(Format::Cbor);
    let borrowed: blob::BlobsRef<'_> = serde_cbor::from_slice(&bytes).expect("borrowed");
    assert!(matches!(borrowed.blobs[0].data, Cow::Borrowed(_)));
}

#[test]
fn integer_map_keys_are_integers_in_cbor_and_strings_in_json() {
    let keyed = families::KeysLongInt::value();
    let first = *keyed.by_id.keys().next().expect("a key");
    let text = String::from_utf8(families::KeysLongInt::bytes(Format::Json)).expect("UTF-8");
    assert!(text.contains(&format!(r#""by_id":{{"{first}":"#)));
    let value: serde_cbor::Value = serde_cbor::from_slice(&families::KeysLongInt::bytes(Format::Cbor)).expect("CBOR");
    let serde_cbor::Value::Map(root) = value else {
        panic!("a map")
    };
    let serde_cbor::Value::Map(by_id) = &root[&serde_cbor::Value::Text("by_id".into())] else {
        panic!("a map")
    };
    assert!(by_id.contains_key(&serde_cbor::Value::Integer(i128::from(first))));
}

#[test]
fn skip_unknown_reads_twitter_like() {
    for format in [Format::Json, Format::Cbor] {
        assert_eq!(world::SkipUnknown::bytes(format), world::TwitterLike::bytes(format));
    }
}

#[test]
fn rejections_damage_json_large_in_one_place() {
    for format in formats() {
        let large = kynos::JsonLarge::bytes(format);
        let truncated = reject::RejectTruncated::bytes(format);
        assert_eq!(truncated.len(), large.len() * 9 / 10, "{format:?}");
        assert!(large.starts_with(&truncated));
    }
    // The shape damage is one item's `id` as text: in CBOR, the integer head
    // replaced by a text head and the digits.
    let large = kynos::JsonLarge::bytes(Format::Cbor);
    let shape = reject::RejectShape::bytes(Format::Cbor);
    let last = (kynos::CATALOG_ITEMS - 1).to_string();
    assert_eq!(shape.len(), large.len() - 3 + 1 + last.len());
    assert!(serde_cbor::from_slice::<serde_cbor::Value>(&shape).is_ok());
    let json = String::from_utf8(reject::RejectShape::bytes(Format::Json)).expect("UTF-8");
    assert_eq!(json.matches(&format!(r#"{{"id":"{last}","#)).count(), 1);
    assert!(reject::RejectShape::bytes(Format::Protobuf).is_empty());
}

#[test]
fn native_formats_carry_what_they_can() {
    for name in ALL {
        let carried = carries_native(name);
        let bytes = on_a_big_stack(move || {
            struct Proto;
            impl Visit for Proto {
                type Output = usize;
                fn visit<W: Workload>(self) -> usize {
                    W::bytes(Format::Protobuf).len()
                }
            }
            dispatch(name, Proto).expect("dispatches")
        });
        assert_eq!(carried, bytes > 0, "{name}");
    }
    assert!(carries_native(kynos::JsonSmall::NAME) && carries_native(kynos::JsonLarge::NAME));
    assert!(carries_native(kynos::EchoPost::NAME));
    assert!(!carries_native(enums::Enums::NAME));
}

/// Every CBOR corpus is preferred serialization (RFC 8949 section 4.1), so
/// no decoder is measured on an encoding a typical client would not send;
/// and every valid one is inside the JSON data model except where the
/// workload is about what is not (byte strings, integer keys).
#[test]
fn cbor_corpora_are_preferred_serialization() {
    for name in ALL {
        let name = *name;
        let bytes = on_a_big_stack(move || {
            struct Cbor;
            impl Visit for Cbor {
                type Output = Vec<u8>;
                fn visit<W: Workload>(self) -> Vec<u8> {
                    W::bytes(Format::Cbor)
                }
            }
            dispatch(name, Cbor).expect("dispatches")
        });
        if name == reject::RejectTruncated::NAME {
            continue;
        }
        assert!(faithful::preferred(&bytes), "{name}");
        let profile = faithful::scan(&bytes).map_err(|r| r.rule);
        match name {
            "byte-strings" => assert_eq!(profile, Err(faithful::Rule::ByteString)),
            "keys-long-int" => assert_eq!(profile, Err(faithful::Rule::NonTextKey)),
            "nested-255" | "nested-257" | "nested-10000" => assert_eq!(profile, Err(faithful::Rule::TooDeep)),
            _ => assert_eq!(profile, Ok(()), "{name}"),
        }
    }
}

#[test]
fn dispatch_refuses_unknown_names() {
    assert_eq!(dispatch("no-such-workload", Facts(Format::Json)), None);
}
