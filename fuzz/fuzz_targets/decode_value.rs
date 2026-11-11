//! Arbitrary bytes into a generic value (`serde_cbor::Value`, through the
//! candidate's `deserialize_any`) and into `IgnoredAny` (which a lazy decoder
//! could skip through without validating): no ill-formed body accepted, and
//! the same value as the oracle wherever both accept.
#![no_main]

use codecs::Backend;
use codecs::backend::serde_cbor::SerdeCbor;
use fuzz::{Check, refuse_ill_formed, same};
use libfuzzer_sys::fuzz_target;
use serde::de::IgnoredAny;

struct DecodeValue;

impl Check for DecodeValue {
    fn check<B: Backend>(data: &[u8]) {
        let body = || bytes::Bytes::copy_from_slice(data);
        let candidate = B::decode::<serde_cbor::Value>(body());
        refuse_ill_formed(data, "value", candidate.is_ok());
        let skipped = B::decode::<IgnoredAny>(body());
        refuse_ill_formed(data, "ignored", skipped.is_ok());
        if let (Ok(candidate), Ok(oracle)) = (candidate, SerdeCbor::decode::<serde_cbor::Value>(body())) {
            assert!(
                same(&candidate, &oracle),
                "value differs from the oracle's: {candidate:?} vs {oracle:?}"
            );
        }
    }
}

fuzz_target!(|data: &[u8]| fuzz::run::<DecodeValue>(data));
