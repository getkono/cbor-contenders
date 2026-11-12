//! An arbitrary value, encoded by the candidate: the output must be one
//! well-formed item the oracle reads as the same value, and the candidate
//! must read the oracle's encoding back as the same value. `to_writer` into
//! Kynos's `BodyWriter` must write exactly what `encode` does.
//!
//! `Value::Null` serializes as serde's unit, which a library may write
//! otherwise than as CBOR `null` (minicbor-serde and cbor4ii write an empty
//! array). That form is the library's, recorded by conformance's `model`
//! section and written as `null` by the worked codec; here a null is
//! expected in the form the backend writes `()` in, decided by encoding it,
//! and counted ([`fuzz::Tally::UnitNotNull`]).
#![no_main]

use arbitrary::{Arbitrary, Unstructured};
use codecs::Backend;
use fuzz::{Check, Tally, same, tally};
use libfuzzer_sys::fuzz_target;
use serde_cbor::Value;

fn value(u: &mut Unstructured<'_>, depth: u32) -> arbitrary::Result<Value> {
    Ok(match u.int_in_range(0..=if depth > 8 { 6 } else { 8 })? {
        0 => Value::Null,
        1 => Value::Bool(bool::arbitrary(u)?),
        2 => Value::Integer(i128::from(i64::arbitrary(u)?)),
        3 => Value::Integer(i128::from(u64::arbitrary(u)?)),
        4 => Value::Float(f64::arbitrary(u)?),
        5 => Value::Text(String::arbitrary(u)?),
        6 => Value::Bytes(Vec::<u8>::arbitrary(u)?),
        7 => Value::Array(
            (0..u.int_in_range(0..=6)?)
                .map(|_| value(u, depth + 1))
                .collect::<arbitrary::Result<_>>()?,
        ),
        _ => {
            let mut map = std::collections::BTreeMap::new();
            for _ in 0..u.int_in_range(0..=6)? {
                map.insert(Value::Text(String::arbitrary(u)?), value(u, depth + 1)?);
            }
            Value::Map(map)
        }
    })
}

/// `value` with every null in the form a backend that writes `()` as an
/// empty array reads back as, and whether it held one.
fn unit_as_empty_array(value: &Value) -> (Value, bool) {
    match value {
        Value::Null => (Value::Array(Vec::new()), true),
        Value::Array(items) => {
            let (items, held): (Vec<_>, Vec<_>) = items.iter().map(unit_as_empty_array).unzip();
            (Value::Array(items), held.contains(&true))
        }
        Value::Map(entries) => {
            let mut held = false;
            let entries = entries
                .iter()
                .map(|(k, v)| {
                    let (v, h) = unit_as_empty_array(v);
                    held |= h;
                    (k.clone(), v)
                })
                .collect();
            (Value::Map(entries), held)
        }
        other => (other.clone(), false),
    }
}

struct Roundtrip;

impl Check for Roundtrip {
    fn check<B: Backend>(data: &[u8]) {
        let Ok(original) = value(&mut Unstructured::new(data), 0) else {
            return;
        };
        let encoded = B::encode(&original);
        // The path a Kynos response takes: `to_writer` into its
        // `BodyWriter`, then `finish`. The same bytes as `encode`, or the
        // same refusal.
        let mut writer = codecs::writer::BodyWriter::new();
        let written = B::to_writer(&original, &mut writer).map(|()| writer.finish());
        match (&encoded, &written) {
            (Ok(encoded), Ok(written)) => assert!(
                encoded[..] == written[..],
                "to_writer into BodyWriter wrote other bytes than encode"
            ),
            (Err(_), Err(_)) => {}
            _ => panic!("encode and to_writer disagree on whether the value encodes: {encoded:?} vs {written:?}"),
        }
        if let Ok(encoded) = encoded {
            assert!(
                faithful::well_formed(&encoded).is_ok(),
                "candidate wrote an ill-formed item"
            );
            let parsed: Value = serde_cbor::from_slice(&encoded).expect("the oracle reads the candidate's output");
            let expected = if B::encode(&()).is_ok_and(|unit| unit == [0x80]) {
                let (expected, held) = unit_as_empty_array(&original);
                if held {
                    tally(Tally::UnitNotNull);
                }
                expected
            } else {
                original.clone()
            };
            assert!(same(&parsed, &expected), "candidate encoded a different value");
        }
        let reference = serde_cbor::to_vec(&original).expect("serde_cbor encodes any Value");
        if let Ok(back) = B::decode::<Value>(bytes::Bytes::from(reference)) {
            assert!(
                same(&back, &original),
                "candidate read the oracle's output as a different value"
            );
        }
    }
}

fuzz_target!(|data: &[u8]| fuzz::run::<Roundtrip>(data));
