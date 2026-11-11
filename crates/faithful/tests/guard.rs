//! The encode half, through serde_cbor (the guard is serializer-agnostic;
//! the worked codec's tests run it through every candidate).

use std::collections::BTreeMap;

use faithful::{Faithful, scan};
use serde::Serialize;

fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
    serde_cbor::to_vec(&Faithful(value)).map_err(|e| e.to_string())
}

#[derive(Serialize)]
struct Record {
    id: u64,
    name: String,
    tags: Vec<String>,
    score: f64,
    parent: Option<u64>,
    labels: BTreeMap<String, u32>,
}

#[test]
fn a_faithful_value_encodes_as_it_would_bare_and_scans_clean() {
    let record = Record {
        id: u64::MAX,
        name: "n".into(),
        tags: vec!["a".into()],
        score: -0.5,
        parent: None,
        labels: BTreeMap::from([("k".into(), 1)]),
    };
    let guarded = encode(&record).expect("faithful");
    assert_eq!(guarded, serde_cbor::to_vec(&record).expect("bare"));
    assert_eq!(scan(&guarded), Ok(()));
    // A `Vec<u8>` without `serde_bytes` is an array of integers, which is
    // what Kynos's schema says it is.
    assert!(encode(&vec![1u8, 2, 3]).is_ok());
}

#[test]
fn values_without_a_json_equivalent_are_refused() {
    for (what, result) in [
        ("bytes", encode(&serde_bytes::ByteBuf::from(vec![1u8]))),
        ("NaN", encode(&f64::NAN)),
        ("infinity f32", encode(&f32::INFINITY)),
        ("u128", encode(&u128::MAX)),
        ("i128", encode(&1i128)),
        ("integer key", encode(&BTreeMap::from([(1u64, 1u32)]))),
        ("bool key", encode(&BTreeMap::from([(true, 1u32)]))),
        ("nested NaN", encode(&vec![Some(vec![1.0, f64::NAN])])),
        (
            "NaN in a map value",
            encode(&BTreeMap::from([("a".to_string(), f64::NAN)])),
        ),
    ] {
        assert!(result.is_err(), "{what} encoded: {result:?}");
    }
}

#[test]
fn string_like_keys_are_accepted() {
    #[derive(Serialize, PartialEq, Eq, PartialOrd, Ord)]
    enum Key {
        Alpha,
    }
    #[derive(Serialize, PartialEq, Eq, PartialOrd, Ord)]
    struct Name(String);
    assert!(encode(&BTreeMap::from([(Key::Alpha, 1u8)])).is_ok());
    assert!(encode(&BTreeMap::from([(Name("x".into()), 1u8)])).is_ok());
    assert!(encode(&BTreeMap::from([('c', 1u8)])).is_ok());
}
