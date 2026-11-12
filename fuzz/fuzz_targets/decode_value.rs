//! Arbitrary bytes into a generic value (`serde_cbor::Value`, through the
//! candidate's `deserialize_any`) and into `IgnoredAny` (which a lazy decoder
//! could skip through without validating): every answer one the body's form
//! allows, and the same value as the oracle wherever both accept (the rule
//! is the crate's).
#![no_main]

use codecs::Backend;
use codecs::backend::serde_cbor::SerdeCbor;
use fuzz::{Check, judge_form, same};
use libfuzzer_sys::fuzz_target;
use serde::de::IgnoredAny;

struct DecodeValue;

impl Check for DecodeValue {
    fn check<B: Backend>(data: &[u8]) {
        let body = || bytes::Bytes::copy_from_slice(data);
        let candidate = B::decode::<serde_cbor::Value>(body());
        let oracle = SerdeCbor::decode::<serde_cbor::Value>(body());
        judge_form::<B, _>(data, "value", &candidate, Some(&oracle));
        let skipped = B::decode::<IgnoredAny>(body()).map(drop);
        let oracle_skipped = SerdeCbor::decode::<IgnoredAny>(body()).map(drop);
        judge_form::<B, _>(data, "ignored", &skipped, Some(&oracle_skipped));
        if let (Ok(candidate), Ok(oracle)) = (candidate, oracle) {
            assert!(
                same(&candidate, &oracle),
                "value differs from the oracle's: {candidate:?} vs {oracle:?}"
            );
        }
    }
}

fuzz_target!(|data: &[u8]| fuzz::run::<DecodeValue>(data));
