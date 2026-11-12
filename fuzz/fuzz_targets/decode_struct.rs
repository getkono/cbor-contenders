//! Arbitrary bytes into a struct using the attributes a server's types use,
//! owned and borrowed: every answer one the body's form allows, and the same
//! value as the oracle and as each other wherever both accept (the rule is the
//! crate's).
//!
//! The owned decode takes the body as a unique buffer and the borrowed one
//! as a frame sharing the connection's buffer.
#![no_main]

use std::borrow::Cow;
use std::collections::BTreeMap;

use codecs::backend::serde_cbor::SerdeCbor;
use codecs::body::Arrival;
use codecs::{Backend, Body};
use fuzz::{Check, Tally, judge_form};
use libfuzzer_sys::fuzz_target;
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Request {
    id: u64,
    #[serde(alias = "label")]
    name: String,
    #[serde(default)]
    tags: Vec<String>,
    score: Option<f64>,
    kind: Kind,
    #[serde(default)]
    extra: BTreeMap<String, i64>,
}

/// [`Request`], borrowing its strings where the input allows.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RequestRef<'a> {
    id: u64,
    #[serde(alias = "label", borrow)]
    name: Cow<'a, str>,
    #[serde(default, borrow)]
    tags: Vec<Cow<'a, str>>,
    score: Option<f64>,
    kind: Kind,
    #[serde(default, borrow)]
    extra: BTreeMap<Cow<'a, str>, i64>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Kind {
    Plain,
    Sized { width: u32, height: u32 },
}

/// The value as the oracle renders it, floats by bits.
fn bits<T: Serialize>(value: &T) -> Option<serde_cbor::Value> {
    serde_cbor::value::to_value(value).ok()
}

/// Whether two decoded values are the same, through the oracle's rendering.
fn same_value<A: Serialize, B: Serialize>(a: &A, b: &B) -> bool {
    matches!((bits(a), bits(b)), (Some(a), Some(b)) if fuzz::same(&a, &b))
}

struct DecodeStruct;

impl Check for DecodeStruct {
    fn check<B: Backend>(data: &[u8]) {
        let owned = B::decode::<Request>(bytes::Bytes::copy_from_slice(data));
        let oracle = SerdeCbor::decode::<Request>(bytes::Bytes::copy_from_slice(data));
        judge_form::<B, _>(data, "owned", &owned, Some(&oracle));
        if let (Ok(c), Ok(r)) = (&owned, &oracle) {
            assert!(same_value(c, r), "owned value differs: {c:?} vs {r:?}");
        }
        let (shared, _connection) = Arrival::Shared.materialize(data);
        let mut body = Body::new(shared);
        let borrowed = B::decode_borrowed::<RequestRef<'_>>(&mut body);
        match (&borrowed, &owned) {
            // A refusal of the borrowed form where the owned one decodes is
            // the candidate not borrowing: loud, and the owned decode is
            // judged above.
            (Err(_), Ok(_)) => fuzz::tally(Tally::NotBorrowed),
            _ => judge_form::<B, _>(data, "borrowed", &borrowed, None),
        }
        if let Ok(b) = &borrowed {
            if let Ok(c) = &owned {
                assert!(same_value(b, c), "borrowed value differs from owned: {b:?} vs {c:?}");
            }
            if let Ok(r) = &oracle {
                assert!(
                    same_value(b, r),
                    "borrowed value differs from the oracle's: {b:?} vs {r:?}"
                );
            }
        }
    }
}

fuzz_target!(|data: &[u8]| fuzz::run::<DecodeStruct>(data));
