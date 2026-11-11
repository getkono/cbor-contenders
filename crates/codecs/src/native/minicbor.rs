//! minicbor 2.3.1's own derive (`#[derive(Encode, Decode)]`, the default
//! array encoding indexed by `#[n]`).
//!
//! A reference point, not a candidate for a typed body: it needs a second
//! derive on every body type, and its wire form (a struct as an array) is not
//! what a JSON Schema object describes, so no schema Kynos derives is true of
//! it. Measured so the cost of the serde layer over the same decoder is known.

use bytes::Bytes;

use crate::{Failure, Format, Native, NativeType};

/// `minicbor::{decode, to_vec, encode}`.
#[derive(Debug)]
pub struct Minicbor;

impl Native for Minicbor {
    const NAME: &'static str = "minicbor";
    const CRATE: &'static str = "minicbor";
    const FORMAT: Format = Format::MinicborNative;
    const ELIGIBLE: bool = false;

    fn decode<T: NativeType>(body: Bytes) -> Result<T, Failure> {
        T::mini_decode(&body)
    }

    fn encode<T: NativeType>(value: &T) -> Result<Vec<u8>, Failure> {
        let mut out = Vec::new();
        value.mini_encode(&mut out)?;
        Ok(out)
    }

    fn encode_into<T: NativeType>(value: &T, out: &mut Vec<u8>) -> Result<(), Failure> {
        out.clear();
        value.mini_encode(out)
    }
}
