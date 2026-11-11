//! An arbitrary value, encoded by the candidate: the output must be one
//! well-formed item the oracle reads as the same value, and the candidate
//! must read the oracle's encoding back as the same value.
#![no_main]

use arbitrary::{Arbitrary, Unstructured};
use codecs::Backend;
use fuzz::{Check, same};
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

struct Roundtrip;

impl Check for Roundtrip {
    fn check<B: Backend>(data: &[u8]) {
        let Ok(original) = value(&mut Unstructured::new(data), 0) else {
            return;
        };
        if let Ok(encoded) = B::encode(&original) {
            assert!(
                faithful::well_formed(&encoded).is_ok(),
                "candidate wrote an ill-formed item"
            );
            let parsed: Value = serde_cbor::from_slice(&encoded).expect("the oracle reads the candidate's output");
            assert!(same(&parsed, &original), "candidate encoded a different value");
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
