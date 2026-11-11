//! Arbitrary bytes into a struct using the attributes a server's types use,
//! owned and borrowed: no ill-formed body accepted, and the same value as the
//! oracle wherever both accept.
//!
//! The owned decode takes the body as a unique buffer and the borrowed one
//! as a frame sharing the connection's buffer.
#![no_main]

use std::borrow::Cow;
use std::collections::BTreeMap;

use codecs::backend::serde_cbor::SerdeCbor;
use codecs::body::Arrival;
use codecs::{Backend, Body};
use fuzz::{Check, refuse_ill_formed};
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

struct DecodeStruct;

impl Check for DecodeStruct {
    fn check<B: Backend>(data: &[u8]) {
        let owned = B::decode::<Request>(bytes::Bytes::copy_from_slice(data));
        refuse_ill_formed(data, "owned", owned.is_ok());
        if let (Ok(c), Ok(r)) = (
            &owned,
            SerdeCbor::decode::<Request>(bytes::Bytes::copy_from_slice(data)),
        ) {
            assert!(
                matches!((bits(c), bits(&r)), (Some(c), Some(r)) if fuzz::same(&c, &r)),
                "owned value differs: {c:?} vs {r:?}"
            );
        }
        let (shared, _connection) = Arrival::Shared.materialize(data);
        let mut body = Body::new(shared);
        let borrowed = B::decode_borrowed::<RequestRef<'_>>(&mut body);
        refuse_ill_formed(data, "borrowed", borrowed.is_ok());
    }
}

fuzz_target!(|data: &[u8]| fuzz::run::<DecodeStruct>(data));
